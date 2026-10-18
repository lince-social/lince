use super::*;

fn profile() -> speech::wake::Profile {
    speech::wake::Profile {
        version: 2,
        threshold: 0.9999,
        references: vec![vec![vec![0.0; 96]; 40]; 3],
        end_padding: vec![4000; 3],
    }
}

#[tokio::test]
async fn wake_setup_persists_and_restarts_off_without_starting_models_or_agents() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("speech.json");
    let host = Host::open(path.clone()).unwrap();
    let initial = host.handle(Request::Inspect).await.unwrap();
    assert!(initial.wake_profile.is_none());
    assert!(!initial.listening.enabled);
    let error = host
        .handle(Request::StartListening {
            mode: ListeningMode::Wake,
            owner: nucleus::new_uid("voice"),
            target: None,
            routing: false,
        })
        .await
        .unwrap_err();
    assert!(error.contains("Record three"));
    let configured = host
        .handle(Request::ConfigureWake {
            profile: Some(profile()),
        })
        .await
        .unwrap();
    assert_eq!(configured.wake_profile.unwrap().references.len(), 3);
    assert!(!configured.listening.enabled);
    assert!(!configured.local.unwrap().loaded);
    host.shutdown().await;
    drop(host);
    let reopened = Host::open(path).unwrap();
    let status = reopened.handle(Request::Inspect).await.unwrap();
    assert!(status.wake_profile.is_some());
    assert!(!status.listening.enabled);
    let cleared = reopened
        .handle(Request::ConfigureWake { profile: None })
        .await
        .unwrap();
    assert!(cleared.wake_profile.is_none());
    reopened.shutdown().await;
}

#[tokio::test]
async fn invalid_optional_wake_profile_remains_recoverable_without_disabling_dictation_settings() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("speech.json");
    std::fs::write(path.with_extension("wake.json"), b"broken profile").unwrap();
    let host = Host::open(path).unwrap();
    let status = host.handle(Request::Inspect).await.unwrap();
    assert!(status.wake_profile.is_none());
    assert!(status.wake_error.is_some());
    assert!(status.local.is_some());
    let recovered = host
        .handle(Request::ConfigureWake {
            profile: Some(profile()),
        })
        .await
        .unwrap();
    assert!(recovered.wake_profile.is_some());
    assert!(recovered.wake_error.is_none());
    let mut invalid = profile();
    invalid.threshold = f32::NAN;
    assert!(
        host.handle(Request::ConfigureWake {
            profile: Some(invalid)
        })
        .await
        .is_err()
    );
    assert!(
        host.handle(Request::Inspect)
            .await
            .unwrap()
            .wake_profile
            .is_some()
    );
    host.shutdown().await;
}
