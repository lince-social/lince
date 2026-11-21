use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::{Artifact, CHUNK_BYTES, Chunk, assemble};
use crate::{
    new_uid,
    trust_analysis::{
        Identity,
        contacts::{Audience, Document, Statement},
    },
};

#[test]
fn assembly_decodes_json_across_a_chunk_boundary() {
    let artifact = Artifact::Contact(Document {
        statement: Statement {
            issuer: Identity::Organ(new_uid("r")),
            contact: Identity::Person(new_uid("r")),
            revision: 1,
            issued_at: 100,
            expires_at: 200,
            withdrawn: false,
            audience: Audience::Public,
        },
        key_id: "test".into(),
        public_key: STANDARD.encode([0; 32]),
        signature: STANDARD.encode([0; 64]),
    });
    let hash = artifact.hash().unwrap();
    let mut wire = vec![b' '; CHUNK_BYTES - 3];
    wire.extend(artifact.bytes().unwrap());
    let total = u32::try_from(wire.len()).unwrap();
    let chunks: Vec<_> = wire
        .chunks(CHUNK_BYTES)
        .enumerate()
        .map(|(index, part)| Chunk {
            hash: hash.clone(),
            offset: u32::try_from(index * CHUNK_BYTES).unwrap(),
            total,
            bytes: STANDARD.encode(part),
        })
        .collect();

    assert_eq!(assemble(&hash, &chunks, 150).unwrap(), artifact);
    assert_eq!(
        assemble(&hash, &chunks[..1], 150).unwrap_err(),
        "Evidence retrieval is incomplete"
    );
}
