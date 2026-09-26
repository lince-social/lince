#![cfg(feature = "native")]

use std::time::{Duration, Instant};

use lince_media::{
    audio::{AudioFrame, FRAME_SAMPLES},
    native::{
        capture::EchoProcessor,
        peer::{Network, Peer, PeerConnectionState, Received, Source, Sources},
    },
    video::VideoFrame,
};

#[tokio::test]
async fn opus_and_av1_cross_native_peers_without_internet_or_capture() {
    loopback(Network::default()).await;
}

#[tokio::test]
#[ignore = "Requires a running TURN server and LINCE_CALL_TURN credentials"]
async fn forced_turn_carries_opus_and_av1() {
    let network = Network::from_environment().unwrap();
    assert!(network.relay_only);
    loopback(network).await;
}

async fn loopback(network: Network) {
    tokio::time::timeout(Duration::from_secs(20), async {
        let sender = Sources::default();
        let receiver = Sources::default();
        for source in [
            Source::Microphone,
            Source::SharedAudio,
            Source::Camera,
            Source::Screen,
        ] {
            assert!(!sender.enabled(source));
            assert!(!receiver.enabled(source));
        }
        let mut a = Peer::new(&sender, &network).unwrap();
        let mut b = Peer::new(&receiver, &network).unwrap();
        let answer = b
            .signal(a.offer(false).await.unwrap())
            .await
            .unwrap()
            .unwrap();
        a.signal(answer).await.unwrap();
        let started = Instant::now();
        loop {
            exchange(&mut a, &mut b).await;
            assert!(a.poll_track().is_none() && b.poll_track().is_none());
            if a.state() == PeerConnectionState::Connected
                && b.state() == PeerConnectionState::Connected
            {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "peer connection did not establish: {:?}, {:?}",
                a.state(),
                b.state()
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        a.set_attendance(6).unwrap();
        b.set_attendance(6).unwrap();
        for source in Source::ALL {
            sender.set_enabled(source, true);
        }
        let image = VideoFrame::new(160, 90, [230, 20, 30, 255].repeat(160 * 90)).unwrap();
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        let mut sent = 0;
        let mut heard = [false; 2];
        let mut seen = [false; 2];
        let screen = VideoFrame::new(160, 90, [20, 30, 230, 255].repeat(160 * 90)).unwrap();
        loop {
            tick.tick().await;
            let samples = std::array::from_fn(|index| {
                (((sent * FRAME_SAMPLES + index) as f32 * 440.0 * std::f32::consts::TAU / 48000.0)
                    .sin()
                    * 8000.0) as i16
            });
            sender.audio(Source::Microphone, samples).await.unwrap();
            sender.audio(Source::SharedAudio, samples).await.unwrap();
            if sent % 10 == 0 {
                sender.video(Source::Camera, &image).unwrap();
                sender.video(Source::Screen, &screen).unwrap();
            }
            sent += 1;
            exchange(&mut a, &mut b).await;
            while let Some((source, frame)) = b.poll_track() {
                match frame {
                    Received::Audio(frame) if source == Source::Microphone => {
                        heard[0] |= frame.0.iter().any(|sample| sample.abs() > 0.01)
                    }
                    Received::Video(frame) if source == Source::Camera => {
                        let rgba = frame.rgba();
                        seen[0] = rgba[0] > 180 && rgba[1] < 60 && rgba[2] < 70;
                    }
                    Received::Audio(frame) if source == Source::SharedAudio => {
                        heard[1] |= frame.0.iter().any(|sample| sample.abs() > 0.01);
                    }
                    Received::Video(frame) if source == Source::Screen => {
                        let rgba = frame.rgba();
                        seen[1] = rgba[2] > 180 && rgba[0] < 60 && rgba[1] < 70;
                    }
                    _ => {}
                }
            }
            if heard == [true; 2] && seen == [true; 2] {
                break;
            }
        }
        sender.set_enabled(Source::Microphone, false);
        sender.set_enabled(Source::Camera, false);
        assert!(!sender.enabled(Source::Microphone));
        assert!(sender.enabled(Source::SharedAudio) && sender.enabled(Source::Screen));
    })
    .await
    .expect("native media loopback timed out");
}

async fn exchange(a: &mut Peer, b: &mut Peer) {
    while let Some(signal) = a.poll_signal().unwrap() {
        b.signal(signal).await.unwrap();
    }
    while let Some(signal) = b.poll_signal().unwrap() {
        a.signal(signal).await.unwrap();
    }
}

#[test]
fn echo_processor_accepts_the_speaker_mix_and_microphone_in_ten_ms_frames() {
    let mut processor = EchoProcessor::default();
    let mut speaker = AudioFrame::silence();
    for (index, sample) in speaker.0.iter_mut().enumerate() {
        *sample = (index as f32 * 0.1).sin() * 0.1;
    }
    for _ in 0..100 {
        processor.speaker(&speaker).unwrap();
        processor.microphone(&speaker, 20).unwrap();
    }
}

#[tokio::test]
async fn reconnect_gathers_fresh_candidates_without_starting_capture() {
    let sources = Sources::default();
    let receiver = Sources::default();
    let mut a = Peer::new(&sources, &Network::default()).unwrap();
    let mut b = Peer::new(&receiver, &Network::default()).unwrap();
    for restart in [false, true] {
        let answer = b
            .signal(a.offer(restart).await.unwrap())
            .await
            .unwrap()
            .unwrap();
        a.signal(answer).await.unwrap();
        let started = Instant::now();
        loop {
            exchange(&mut a, &mut b).await;
            assert!(a.poll_track().is_none() && b.poll_track().is_none());
            if a.state() == PeerConnectionState::Connected
                && b.state() == PeerConnectionState::Connected
            {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "reconnect did not establish"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        for source in Source::ALL {
            assert!(!sources.enabled(source) && !receiver.enabled(source));
        }
    }
}

#[tokio::test]
async fn mismatched_certificate_fingerprint_never_admits_media() {
    use lince_media::native::peer::Signal;
    let sources = Sources::default();
    let receiver = Sources::default();
    let mut a = Peer::new(&sources, &Network::default()).unwrap();
    let mut b = Peer::new(&receiver, &Network::default()).unwrap();
    let Signal::Offer(offer) = a.offer(false).await.unwrap() else {
        panic!("expected offer")
    };
    let forged = offer
        .lines()
        .map(|line| {
            if line.starts_with("a=fingerprint:") {
                format!("a=fingerprint:sha-256 {}", ["00"; 32].join(":"))
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n";
    let answer = b.signal(Signal::Offer(forged)).await.unwrap().unwrap();
    a.signal(answer).await.unwrap();
    sources.set_enabled(Source::Microphone, true);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(1) {
        while let Ok(Some(signal)) = a.poll_signal() {
            b.signal(signal).await.unwrap();
        }
        while let Ok(Some(signal)) = b.poll_signal() {
            a.signal(signal).await.unwrap();
        }
        sources
            .audio(Source::Microphone, [8000; FRAME_SAMPLES])
            .await
            .unwrap();
        assert!(b.poll_track().is_none());
        assert_ne!(b.state(), PeerConnectionState::Connected);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
