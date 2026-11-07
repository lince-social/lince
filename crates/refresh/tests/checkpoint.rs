#![cfg(target_os = "linux")]

use lince_refresh::{
    checkpoint::{
        CheckpointFile, DescriptorPurpose, DescriptorSpec, Envelope, ExecutableIdentity,
        HeldCheckpoint,
    },
    transport,
};
use std::{
    fs::File,
    io::Write,
    os::{
        fd::{AsFd, OwnedFd},
        unix::fs::FileExt,
    },
    time::{Duration, Instant},
};

fn envelope(descriptors: Vec<DescriptorSpec>) -> Envelope {
    let identity = ExecutableIdentity {
        revision: Some("a".repeat(40)),
        sha256: "b".repeat(64),
        device: 1,
        inode: 2,
        target: "x86_64-unknown-linux-gnu".into(),
    };
    Envelope {
        schema: 1,
        scope: lince_refresh::checkpoint::CheckpointScope::ProtocolExperiment,
        transaction: "c".repeat(64),
        source: identity.clone(),
        candidate: identity,
        source_generation: 7,
        candidate_generation: 8,
        descriptors,
        payload_length: 0,
        payload_sha256: String::new(),
    }
}

#[test]
fn sealed_checkpoint_rejects_mutation_and_retains_exact_payload() {
    let checkpoint = CheckpointFile::create(envelope(Vec::new()), b"private typed state").unwrap();
    let mut file = File::from(checkpoint.descriptor().try_clone_to_owned().unwrap());
    assert!(file.write_all(b"changed").is_err());
    assert!(file.set_len(0).is_err());
    let restored =
        CheckpointFile::open(checkpoint.descriptor().try_clone_to_owned().unwrap()).unwrap();
    assert_eq!(restored.payload().unwrap(), b"private typed state");
    assert_eq!(restored.envelope().source_generation, 7);
    assert_eq!(restored.envelope().candidate_generation, 8);
}

#[test]
fn checkpoint_rejects_an_unsealed_file_and_an_invalid_generation() {
    let file = tempfile::tempfile().unwrap();
    assert!(CheckpointFile::open(file.into()).is_err());
    let mut invalid = envelope(Vec::new());
    invalid.candidate_generation = 9;
    assert!(CheckpointFile::create(invalid, b"state").is_err());
}

#[test]
fn descriptor_transfer_preserves_order_sharing_contents_and_close_on_exec_across_chunks() {
    let file = tempfile::tempfile().unwrap();
    file.set_len(64).unwrap();
    file.write_all_at(b"before", 0).unwrap();
    let resources = (0..257)
        .map(|_| OwnedFd::from(file.try_clone().unwrap()))
        .collect::<Vec<_>>();
    let table = resources
        .iter()
        .enumerate()
        .map(|(index, descriptor)| {
            DescriptorSpec::capture(
                DescriptorPurpose::ShmPool,
                format!("pool/{index}"),
                descriptor,
            )
            .unwrap()
        })
        .collect();
    let checkpoint =
        CheckpointFile::create(envelope(table), b"payload with preserved references").unwrap();
    let transaction = checkpoint.envelope().transaction.clone();
    let held = HeldCheckpoint::new(checkpoint, resources).unwrap();
    let (source, target) = transport::pair().unwrap();
    let source_peer = transport::peer(&source).unwrap();
    let target_peer = transport::peer(&target).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let sender = std::thread::spawn(move || transport::send(source, source_peer, &held, deadline));
    let received = transport::receive(target, target_peer, &transaction, deadline).unwrap();
    sender.join().unwrap().unwrap();
    assert_eq!(received.descriptors.len(), 257);
    assert_eq!(
        received.checkpoint.payload().unwrap(),
        b"payload with preserved references"
    );
    for descriptor in &received.descriptors {
        assert!(
            rustix::io::fcntl_getfd(descriptor)
                .unwrap()
                .contains(rustix::io::FdFlags::CLOEXEC)
        );
        let file = File::from(descriptor.try_clone().unwrap());
        let mut bytes = [0_u8; 6];
        file.read_exact_at(&mut bytes, 0).unwrap();
        assert_eq!(&bytes, b"before");
    }
    file.write_all_at(b"after!", 0).unwrap();
    let shared = File::from(received.descriptors[128].try_clone().unwrap());
    let mut bytes = [0_u8; 6];
    shared.read_exact_at(&mut bytes, 0).unwrap();
    assert_eq!(&bytes, b"after!");
}

#[test]
fn checkpoint_rejects_a_resource_with_the_wrong_identity() {
    let source = tempfile::tempfile().unwrap();
    let other = tempfile::tempfile().unwrap();
    let table = vec![
        DescriptorSpec::capture(DescriptorPurpose::ShmPool, "pool/1".into(), &source).unwrap(),
    ];
    let checkpoint = CheckpointFile::create(envelope(table), b"state").unwrap();
    assert!(HeldCheckpoint::new(checkpoint, vec![other.into()]).is_err());
}

#[test]
fn empty_descriptor_transfer_still_authenticates_its_transaction() {
    let checkpoint = CheckpointFile::create(envelope(Vec::new()), b"state").unwrap();
    let held = HeldCheckpoint::new(checkpoint, Vec::new()).unwrap();
    let (source, target) = transport::pair().unwrap();
    let source_peer = transport::peer(&source).unwrap();
    let target_peer = transport::peer(&target).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let sender = std::thread::spawn(move || transport::send(source, source_peer, &held, deadline));
    assert!(transport::receive(target, target_peer, &"d".repeat(64), deadline).is_err());
    let _ = sender.join().unwrap();
}

#[test]
fn empty_transport_has_a_bounded_transaction_deadline() {
    let (_source, target) = transport::pair().unwrap();
    let expected = transport::peer(&target).unwrap();
    let result = transport::receive(
        &target,
        expected,
        &"c".repeat(64),
        Instant::now() + Duration::from_millis(30),
    );
    assert_eq!(result.err().unwrap().kind(), std::io::ErrorKind::TimedOut);
    assert!(
        rustix::io::fcntl_getfd(target.as_fd())
            .unwrap()
            .contains(rustix::io::FdFlags::CLOEXEC)
    );
}
