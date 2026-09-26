#![cfg(feature = "native")]

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use lince_media::{
    audio::FRAME_SAMPLES,
    native::peer::{Network, Peer, PeerConnectionState, Received, Source, Sources},
};

#[tokio::test]
async fn six_devices_exchange_voice_over_fifteen_mesh_links() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let sources: Vec<_> = (0..6).map(|_| Sources::default()).collect();
        let mut peers = BTreeMap::new();
        for left in 0..6 {
            for right in left + 1..6 {
                let mut a = Peer::new(&sources[left], &Network::default()).unwrap();
                let mut b = Peer::new(&sources[right], &Network::default()).unwrap();
                a.set_attendance(6).unwrap();
                b.set_attendance(6).unwrap();
                let offer = a.offer(false).await.unwrap();
                let answer = b.signal(offer).await.unwrap().unwrap();
                a.signal(answer).await.unwrap();
                peers.insert((left, right), a);
                peers.insert((right, left), b);
            }
        }
        let connecting = std::time::Instant::now();
        loop {
            let mut signals = Vec::new();
            for ((from, to), peer) in &mut peers {
                while let Some(signal) = peer.poll_signal().unwrap() {
                    signals.push((*to, *from, signal));
                }
            }
            for (to, from, signal) in signals {
                peers
                    .get_mut(&(to, from))
                    .unwrap()
                    .signal(signal)
                    .await
                    .unwrap();
            }
            if peers
                .values()
                .all(|peer| peer.state() == PeerConnectionState::Connected)
            {
                break;
            }
            assert!(
                connecting.elapsed() < Duration::from_secs(10),
                "connections: {:?}",
                peers
                    .iter()
                    .map(|(pair, peer)| (pair, peer.state()))
                    .collect::<Vec<_>>()
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        for source in &sources {
            source.set_enabled(Source::Microphone, true);
        }
        let mut heard = BTreeSet::new();
        for tick in 0..200 {
            for (device, source) in sources.iter().enumerate() {
                let pcm = std::array::from_fn(|index| {
                    (((tick * FRAME_SAMPLES + index) as f32
                        * (380.0 + device as f32 * 30.0)
                        * std::f32::consts::TAU
                        / 48_000.0)
                        .sin()
                        * 5000.0) as i16
                });
                source.audio(Source::Microphone, pcm).await.unwrap();
            }
            let mut signals = Vec::new();
            for ((from, to), peer) in &mut peers {
                while let Some(signal) = peer.poll_signal().unwrap() {
                    signals.push((*to, *from, signal));
                }
                while let Some((source, frame)) = peer.poll_track() {
                    if let Received::Audio(frame) = frame {
                        if source == Source::Microphone
                            && frame.0.iter().any(|sample| sample.abs() > 0.01)
                        {
                            heard.insert((*from, *to));
                        }
                    }
                }
            }
            for (to, from, signal) in signals {
                peers
                    .get_mut(&(to, from))
                    .unwrap()
                    .signal(signal)
                    .await
                    .unwrap();
            }
            if heard.len() == 30 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            heard.len(),
            30,
            "every device must hear all five other devices"
        );
    })
    .await
    .expect("six-device mesh timed out");
}
