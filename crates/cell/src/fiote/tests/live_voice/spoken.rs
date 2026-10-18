use super::*;
use crate::speech::{Host as SpeechHost, Request as SpeechRequest, Service as _};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use fiote::speech::{ListeningMode, ListeningStage, ListeningStatus, voice};
use std::time::{Duration, Instant};

async fn speech_job(host: &SpeechHost, job: &str) {
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let result = host
                .handle(SpeechRequest::Poll { job: job.into() })
                .await
                .unwrap()
                .job
                .unwrap();
            if !result.pending {
                assert!(result.error.is_none(), "{:?}", result.error);
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
}

async fn listening(
    host: &SpeechHost,
    expected: impl Fn(&ListeningStatus) -> bool,
) -> ListeningStatus {
    tokio::time::timeout(Duration::from_secs(180), async {
        let started = Instant::now();
        let mut previous = None;
        loop {
            let status = host.handle(SpeechRequest::Inspect).await.unwrap().listening;
            if previous != Some(status.stage) {
                println!(
                    "speech_stage={:?} elapsed_ms={}",
                    status.stage,
                    started.elapsed().as_millis()
                );
                previous = Some(status.stage);
            }
            assert_ne!(status.stage, ListeningStage::Degraded, "{status:?}");
            if expected(&status) {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Consumes native provider usage; requires protected connection environment, LINCE_SPEECH_TEST_CACHE and LINCE_VOICE_TEST_ASSETS; uses frozen synthetic WAVs after acoustic activation, without physical capture"]
async fn actual_recorded_speech_uses_native_conversation_followups_closing_and_synthesis() {
    let (mut settings, key) = existing_connection().await;
    let setup_started = Instant::now();
    let (base, _script, root, record, _) = fixture(false).await;
    settings.directory = root.path().join("workspace");
    std::fs::create_dir(&settings.directory).unwrap();
    settings.enabled = true;
    base.vault.put(vault::slot(&settings), key).await.unwrap();
    base.configure(&record, settings, None, None).await.unwrap();
    let mut host = base.as_ref().clone();
    host.provider = None;
    let host = Arc::new(host);
    let speech = SpeechHost::open(root.path().join("speech.json"))
        .unwrap()
        .with_fiote(Some(host.clone()));
    let audio_root = std::env::var_os("LINCE_FIOTE_VOICE_TEST_AUDIO")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fiote/tests/fixtures/conversation")
        });
    let references: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../fiote/tests/fixtures/english-conversation-reference.json"
    ))
    .unwrap();
    let audio = |name: &str| {
        let bytes = std::fs::read(audio_root.join(format!("{name}.wav"))).unwrap();
        assert_eq!(
            nucleus::message::digest(&bytes),
            references["cases"][name]["sha256"].as_str().unwrap(),
            "Choose the frozen public conversation recordings"
        );
        let audio = Secret(BASE64.encode(bytes));
        fiote::speech::validate_audio(&audio.0).unwrap();
        audio
    };
    let references = ["wake-statement", "wake-question", "wake-exclamation"]
        .map(|name| fiote::speech::decode_audio(&audio(name).0).unwrap());
    speech
        .handle(SpeechRequest::ConfigureWake {
            profile: Some(fiote::speech::wake::Profile::enroll(&references).unwrap()),
        })
        .await
        .unwrap();
    let local = speech
        .handle(SpeechRequest::Inspect)
        .await
        .unwrap()
        .local
        .unwrap();
    let archive = PathBuf::from(std::env::var_os("LINCE_SPEECH_TEST_CACHE").unwrap())
        .join(format!("{}-{}", local.model, local.revision))
        .join("phonon-2.bps.tar.zst");
    let setup = nucleus::new_uid("speech");
    speech
        .handle(SpeechRequest::InstallLocal {
            job: setup.clone(),
            source: Some(archive),
        })
        .await
        .unwrap();
    speech_job(&speech, &setup).await;
    println!("local_setup_ms={}", setup_started.elapsed().as_millis());
    let owner = nucleus::new_uid("voice");
    let started = speech
        .handle(SpeechRequest::StartListening {
            mode: ListeningMode::Wake,
            owner: owner.clone(),
            target: Some(record.clone()),
            routing: false,
        })
        .await
        .unwrap();
    assert!(!started.decision_model.unwrap().present);
    let generation = started.listening.generation;
    let first_started = Instant::now();
    speech
        .handle(SpeechRequest::WokenSegment {
            owner: owner.clone(),
            generation,
            segment: 1,
            audio: audio("first"),
        })
        .await
        .unwrap();
    let first = listening(&speech, |status| status.receipt.is_some())
        .await
        .receipt
        .unwrap();
    let first = completed(&host, &first.request_id).await;
    println!(
        "first_request_completed_ms={}",
        first_started.elapsed().as_millis()
    );
    let message = host
        .record(first.message.as_deref().unwrap())
        .await
        .unwrap();
    assert!(
        message.body.to_lowercase().contains("sapphire"),
        "{}",
        message.body
    );
    let followup_started = Instant::now();
    speech
        .handle(SpeechRequest::ListeningSegment {
            owner: owner.clone(),
            generation,
            segment: 2,
            audio: audio("followup"),
        })
        .await
        .unwrap();
    let second = listening(&speech, |status| {
        status
            .receipt
            .as_ref()
            .is_some_and(|receipt| receipt.request_id != first.request_id)
    })
    .await
    .receipt
    .unwrap();
    let second = completed(&host, &second.request_id).await;
    println!(
        "followup_completed_ms={}",
        followup_started.elapsed().as_millis()
    );
    assert_eq!(second.target, record);
    assert_eq!(second.thread, first.thread);
    let reply = host.record(second.reply.as_deref().unwrap()).await.unwrap();
    assert!(
        reply.body.to_lowercase().contains("sapphire"),
        "{}",
        reply.body
    );
    speech
        .handle(SpeechRequest::PauseListening {
            owner: owner.clone(),
            generation,
        })
        .await
        .unwrap();
    assert!(
        speech
            .handle(SpeechRequest::ListeningSegment {
                owner: owner.clone(),
                generation,
                segment: 3,
                audio: audio("background"),
            })
            .await
            .is_err()
    );
    let setup = nucleus::new_uid("speech");
    speech
        .handle(SpeechRequest::Voice {
            request: voice::Request::Install {
                job: setup.clone(),
                source: Some(PathBuf::from(
                    std::env::var_os("LINCE_VOICE_TEST_ASSETS").unwrap(),
                )),
            },
        })
        .await
        .unwrap();
    let spoken = nucleus::new_uid("speech");
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let status = speech
                .handle(SpeechRequest::Voice {
                    request: voice::Request::Poll { job: setup.clone() },
                })
                .await
                .unwrap();
            let job = status.voice.job.unwrap();
            if !job.pending {
                assert!(job.error.is_none(), "{:?}", job.error);
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let synthesis_started = Instant::now();
    speech
        .handle(SpeechRequest::Voice {
            request: voice::Request::Speak {
                job: spoken.clone(),
                text: reply.body,
            },
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let job = speech
                .handle(SpeechRequest::Voice {
                    request: voice::Request::Poll {
                        job: spoken.clone(),
                    },
                })
                .await
                .unwrap()
                .voice
                .job
                .unwrap();
            if !job.pending {
                assert!(job.error.is_none(), "{:?}", job.error);
                let wav = BASE64.decode(&job.audio.unwrap().0).unwrap();
                assert!(wav.len() > 4800 && wav.starts_with(b"RIFF") && &wav[8..12] == b"WAVE");
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    println!(
        "first_reply_audio_ready_ms={}",
        synthesis_started.elapsed().as_millis()
    );
    let resumed = speech
        .handle(SpeechRequest::ResumeListening {
            owner: owner.clone(),
            generation,
        })
        .await
        .unwrap();
    assert!(resumed.listening.generation > generation);
    assert!(
        speech
            .handle(SpeechRequest::ListeningSegment {
                owner: owner.clone(),
                generation,
                segment: 2,
                audio: audio("followup"),
            })
            .await
            .is_err()
    );
    let generation = resumed.listening.generation;
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&host.engine.store.pool)
        .await
        .unwrap();
    speech
        .handle(SpeechRequest::ListeningSegment {
            owner: owner.clone(),
            generation,
            segment: 4,
            audio: audio("ending"),
        })
        .await
        .unwrap();
    let ended = listening(&speech, |status| {
        status.stage == ListeningStage::Ignored && status.conversation.is_none()
    })
    .await;
    assert_eq!(ended.receipt.unwrap().request_id, second.request_id);
    speech
        .handle(SpeechRequest::ListeningSegment {
            owner: owner.clone(),
            generation,
            segment: 5,
            audio: audio("background"),
        })
        .await
        .unwrap();
    listening(&speech, |status| {
        !matches!(
            status.stage,
            ListeningStage::Transcribing | ListeningStage::Deciding
        )
    })
    .await;
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&host.engine.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    speech
        .handle(SpeechRequest::WokenSegment {
            owner: owner.clone(),
            generation,
            segment: 6,
            audio: audio("first"),
        })
        .await
        .unwrap();
    let reopened = listening(&speech, |status| {
        status
            .receipt
            .as_ref()
            .is_some_and(|receipt| receipt.request_id != second.request_id)
    })
    .await
    .receipt
    .unwrap();
    let reopened = completed(&host, &reopened.request_id).await;
    assert_ne!(reopened.thread, first.thread);
    speech
        .handle(SpeechRequest::StopListening { owner, generation })
        .await
        .unwrap();
    let resources = || {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .map(|status| {
                status
                    .lines()
                    .filter(|line| line.starts_with("VmRSS:") || line.starts_with("VmHWM:"))
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
    };
    println!("loaded_resources={:?}", resources());
    speech.shutdown().await;
    host.stop_all().await;
    println!("shutdown_resources={:?}", resources());
    println!(
        "Real ASR and native provider: remembered speech, same-thread contextual follow-up, synthesized reply, paused-capture refusal, natural closing without messages, background refusal and a fresh activated thread."
    );
}
