//! First contact (Ontology §11, settled 2026-08-03).
//!
//! Only the ACQUISITION of a NodeId is ever at risk. Once you hold one,
//! dialing it reaches that keypair or nothing — under iroh the address IS the
//! key, so there is no wire left to substitute on. That single fact is what
//! decides the whole shape of this module.
//!
//! So the ranked flows are:
//!   a. **A QR code scanned in person.** The visual channel cannot be relayed
//!      and you can see who you are handing it to. Best available.
//!   b. **Pasted into a messaging app you already trust.** Equally strong —
//!      that channel is already authenticated to that human.
//!   c. Discovery plus a conversation — weakest, because a live relay passes
//!      a conversational challenge unharmed. Acceptable only because (a) and
//!      (b) cover the flows this product actually has.
//!
//! The verification code is deliberately NOT in any of these. It defended
//! remote first contact with no other trusted channel, and a security step
//! users are taught to click past is worse than no step. It survives as an
//! optional panel; see `peers::verification_code`.
//!
//! An invite carries ADDRESSES as well as the NodeId, which is what makes an
//! in-person exchange work with no discovery mechanism at all — the case that
//! matters on guest wifi, in hotels, and on corporate networks where mDNS is
//! blocked.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;

use crate::error::EngineError;

/// Version tag. A future format bumps this and old readers refuse rather than
/// guess — failing closed on the unknown, as everywhere else.
const INVITE_PREFIX: &str = "lince1";

/// Field separator. `|` cannot occur in a NodeId (base32), in standard base64,
/// or in an `ip:port`, and the label is base64-encoded precisely so that a
/// name containing one cannot break the framing.
const SEP: char = '|';

/// Everything one Organ needs to reach and recognise another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingInvite {
    pub node_id: String,
    /// The Organ ROOT public key. Adopting it here is the ONE
    /// trust-on-first-use in the design — every later roster and succession
    /// must chain from it. That is why it belongs in the QR: acquiring it
    /// over an unrelayable channel is what the whole chain rule rests on.
    pub root_key: Option<String>,
    /// A self-declared label. UNTRUSTED, and the receiving UI must let the
    /// local user type their own name rather than adopting this one.
    pub label: Option<String>,
    /// Direct addresses, so an in-person scan needs no discovery at all.
    pub addrs: Vec<String>,
}

impl PairingInvite {
    pub fn encode(&self) -> String {
        format!(
            "{INVITE_PREFIX}{SEP}{}{SEP}{}{SEP}{}{SEP}{}",
            self.node_id,
            self.root_key.clone().unwrap_or_default(),
            self.label
                .as_deref()
                .map(|label| B64.encode(label.as_bytes()))
                .unwrap_or_default(),
            self.addrs.join(",")
        )
    }

    pub fn decode(text: &str) -> Result<PairingInvite, EngineError> {
        let text = text.trim();
        let parts: Vec<&str> = text.split(SEP).collect();
        if parts.len() != 5 || parts[0] != INVITE_PREFIX {
            // Name the field they most likely copied from instead. The Profile
            // panel shows two strings and only this one is pasteable, so "not a
            // pairing code" alone leaves them re-pasting the same wrong one.
            return Err(EngineError::Consequence(
                "not a Lince pairing code. It must be the whole line starting `lince1|` \
                 from under their QR code — an identity key on its own carries no \
                 address and cannot be added."
                    .into(),
            ));
        }
        if parts[1].is_empty() {
            return Err(EngineError::Consequence(
                "pairing code carries no node id".into(),
            ));
        }
        let label = if parts[3].is_empty() {
            None
        } else {
            B64.decode(parts[3])
                .ok()
                .and_then(|raw| String::from_utf8(raw).ok())
        };
        Ok(PairingInvite {
            node_id: parts[1].to_string(),
            root_key: (!parts[2].is_empty()).then(|| parts[2].to_string()),
            label,
            addrs: parts[4]
                .split(',')
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect(),
        })
    }

    /// The invite as an SVG QR code, for showing on screen.
    ///
    /// Rendered server-side because the sand runs under a CSP that blocks
    /// every external script — vendoring a QR encoder into the browser would
    /// be a second implementation of something already needed here.
    pub fn qr_svg(&self) -> Result<String, EngineError> {
        use qrcode::QrCode;
        use qrcode::render::svg;
        let code = QrCode::new(self.encode().as_bytes())
            .map_err(|error| EngineError::Consequence(format!("QR encode failed: {error}")))?;
        Ok(code
            .render()
            .min_dimensions(220, 220)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .build())
    }
}

/// Version tag for a MAILBOX invite. A third prefix, for the same reason there
/// is a second: these three codes are shown in the same shape and scanned by
/// the same camera, and they grant wildly different things — a contact, a
/// device of your own identity, and the right to leave sealed bytes on
/// somebody's disk. None may ever be read as another by accident.
const MAILBOX_PREFIX: &str = "lincemail1";

/// "You may leave your mail with me."
///
/// Two fields and no more: whom to ask, and the single-use token that says the
/// operator meant it. No root key, unlike the enrolment code — the redeemer is
/// not joining an identity and has nothing to verify about the carrier beyond
/// its node id, which iroh authenticates by construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxInviteCode {
    pub node_id: String,
    pub token: String,
}

impl MailboxInviteCode {
    pub fn encode(&self) -> String {
        format!("{MAILBOX_PREFIX}{SEP}{}{SEP}{}", self.node_id, self.token)
    }

    pub fn decode(text: &str) -> Result<MailboxInviteCode, EngineError> {
        let parts: Vec<&str> = text.trim().split(SEP).collect();
        if parts.len() != 3 || parts[0] != MAILBOX_PREFIX {
            return Err(EngineError::Consequence(
                "not a Lince mailbox invite. It must be the whole line starting \
                 `lincemail1|`, from someone offering to hold your mail — a pairing \
                 code adds a contact and an enrolment code adds a device, and neither \
                 can do this."
                    .into(),
            ));
        }
        if parts[1].is_empty() || parts[2].is_empty() {
            return Err(EngineError::Consequence(
                "that mailbox invite is missing part of itself".into(),
            ));
        }
        Ok(MailboxInviteCode {
            node_id: parts[1].to_string(),
            token: parts[2].to_string(),
        })
    }
}

/// Version tag for an ENROLMENT code. A separate prefix from `lince1`, and
/// that separation is the point: a pairing code adds a contact, an enrolment
/// code adds a device to your own identity. They are shown in the same shape,
/// scanned by the same camera, and one of them grants strictly more than the
/// other, so neither may ever be read as the other by accident.
const ENROLMENT_PREFIX: &str = "lincecell1";

/// Everything a NEW DEVICE needs to join an existing Organ.
///
/// Enrolling is pairing with YOURSELF, and it earns its own flow rather than
/// reusing contact pairing (Ontology §11). The single-use, short-lived token
/// is what makes it safe to show on a screen: it grants membership in an
/// identity, which is strictly more than a contact code grants, so it expires
/// in minutes and works exactly once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrolmentInvite {
    /// The enrolling Cell — an existing member that holds the root.
    pub node_id: String,
    /// The Organ being joined. Carried so the new Cell knows what identity it
    /// is about to become part of BEFORE it connects, and so the roster it
    /// gets back can be checked against what it was offered.
    pub organ_uid: String,
    /// The Organ root public key, adopted by the joining Cell as its own
    /// identity's root. Same trust-on-first-use as pairing, over the same
    /// unrelayable visual channel.
    pub root_key: String,
    /// Single-use, minutes-long. `roster::ENROLMENT_TOKEN_TTL_MINUTES`.
    pub token: String,
    /// Direct addresses, so enrolment works with no discovery at all — the
    /// case that matters on guest wifi and in hotels, exactly as for pairing.
    pub addrs: Vec<String>,
}

impl EnrolmentInvite {
    pub fn encode(&self) -> String {
        format!(
            "{ENROLMENT_PREFIX}{SEP}{}{SEP}{}{SEP}{}{SEP}{}{SEP}{}",
            self.node_id,
            self.organ_uid,
            self.root_key,
            self.token,
            self.addrs.join(",")
        )
    }

    pub fn decode(text: &str) -> Result<EnrolmentInvite, EngineError> {
        let parts: Vec<&str> = text.trim().split(SEP).collect();
        if parts.len() != 6 || parts[0] != ENROLMENT_PREFIX {
            return Err(EngineError::Consequence(
                "not a Lince enrolment code. It must be the whole line starting \
                 `lincecell1|` from the Add a device panel on a Cell you already \
                 own — a pairing code adds a contact and cannot enrol a device."
                    .into(),
            ));
        }
        // Every field is load-bearing: without the node id there is nobody to
        // ask, without the uid and key there is no identity to verify the
        // answer against, and without the token the ask is refused.
        for (index, what) in [(1, "node id"), (2, "organ"), (3, "root key"), (4, "token")] {
            if parts[index].is_empty() {
                return Err(EngineError::Consequence(format!(
                    "enrolment code carries no {what}"
                )));
            }
        }
        Ok(EnrolmentInvite {
            node_id: parts[1].to_string(),
            organ_uid: parts[2].to_string(),
            root_key: parts[3].to_string(),
            token: parts[4].to_string(),
            addrs: parts[5]
                .split(',')
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect(),
        })
    }

    /// The invite as an SVG QR code, for showing on the enrolling Cell's
    /// screen. Same reasoning as [`PairingInvite::qr_svg`].
    pub fn qr_svg(&self) -> Result<String, EngineError> {
        use qrcode::QrCode;
        use qrcode::render::svg;
        let code = QrCode::new(self.encode().as_bytes())
            .map_err(|error| EngineError::Consequence(format!("QR encode failed: {error}")))?;
        Ok(code
            .render()
            .min_dimensions(220, 220)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .build())
    }
}

/// Read a QR code out of a captured camera frame.
///
/// The mirror of [`PairingInvite::qr_svg`], and here for the same reason plus
/// one more. The same: a sand runs under a CSP that blocks every external
/// script, so a decoder in the browser would be a second implementation of
/// something already needed on this side. The one more: what comes out of a
/// decoder is a PAIRING CODE — a thing that decides who this Cell trusts — so
/// it belongs in one audited place rather than in every sand that scans.
///
/// `Ok(None)` means "no code in this frame", which is the ORDINARY answer
/// while a camera is pointed at a wall and must not be an error. `Err` is
/// reserved for a frame that could not be read as an image at all.
pub fn decode_qr(frame: &[u8]) -> Result<Option<String>, EngineError> {
    let image = image::load_from_memory(frame)
        .map_err(|error| EngineError::Consequence(format!("unreadable image: {error}")))?
        .to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(image);
    for grid in prepared.detect_grids() {
        // A grid that fails to decode is a partial or damaged sighting, not a
        // failure of the request: keep looking at the others.
        if let Ok((_meta, text)) = grid.decode() {
            return Ok(Some(text));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_field() {
        let invite = PairingInvite {
            node_id: "abc123".into(),
            root_key: Some("aGVsbG8gd29ybGQ=".into()),
            label: Some("Eduardo's laptop | office".into()),
            addrs: vec!["192.168.1.5:4433".into(), "[::1]:4433".into()],
        };
        let decoded = PairingInvite::decode(&invite.encode()).expect("decode");
        assert_eq!(decoded, invite, "a label containing the separator survives");
    }

    #[test]
    fn a_minimal_invite_round_trips() {
        let invite = PairingInvite {
            node_id: "abc123".into(),
            root_key: None,
            label: None,
            addrs: Vec::new(),
        };
        assert_eq!(
            PairingInvite::decode(&invite.encode()).expect("decode"),
            invite
        );
    }

    #[test]
    fn foreign_or_malformed_text_is_refused_not_guessed() {
        for bad in [
            "",
            "hello",
            "lince2|abc||",
            "lince1|||",
            "lince1||key|bGFiZWw=|addr",
        ] {
            assert!(
                PairingInvite::decode(bad).is_err(),
                "must refuse {bad:?} rather than guess"
            );
        }
    }

    /// Rasterize a QR the way a camera would see one: each module a solid
    /// block, with the quiet zone the spec requires — a decoder given a code
    /// cropped flush to its edge finds nothing.
    ///
    /// Hand-rolled rather than using `qrcode`'s `image` feature, which pins an
    /// older `image` than the decoder uses and would put two copies of it in
    /// the tree.
    fn rasterize(text: &str) -> Vec<u8> {
        use image::{GrayImage, Luma};
        const SCALE: u32 = 8;
        const QUIET: u32 = 4;

        let code = qrcode::QrCode::new(text.as_bytes()).expect("encode");
        let colors = code.to_colors();
        let modules = code.width() as u32;
        let side = (modules + QUIET * 2) * SCALE;
        let mut image = GrayImage::from_pixel(side, side, Luma([255u8]));
        for y in 0..modules {
            for x in 0..modules {
                if colors[(y * modules + x) as usize] != qrcode::Color::Dark {
                    continue;
                }
                for dy in 0..SCALE {
                    for dx in 0..SCALE {
                        image.put_pixel(
                            (x + QUIET) * SCALE + dx,
                            (y + QUIET) * SCALE + dy,
                            Luma([0u8]),
                        );
                    }
                }
            }
        }
        let mut png = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("encode png");
        png.into_inner()
    }

    /// The scan path end to end: what this Cell renders is what a camera
    /// pointed at it decodes back, and what comes out is a usable invite.
    #[test]
    fn a_rendered_invite_survives_being_photographed_and_read_back() {
        let invite = PairingInvite {
            node_id: "k5nqx7dtqz3vf2mhb4wc6yjr8plsgan9euo1i0t".into(),
            root_key: Some("aGVsbG8gd29ybGQ=".into()),
            label: Some("Eduardo's laptop | office".into()),
            addrs: vec!["192.168.1.5:4433".into()],
        };

        let decoded = decode_qr(&rasterize(&invite.encode()))
            .expect("a rendered code is a readable image")
            .expect("and it contains a code");

        assert_eq!(
            PairingInvite::decode(&decoded).expect("decodes as an invite"),
            invite,
            "scanning must return the same invite that was rendered, label \
             separator and all"
        );
    }

    #[test]
    fn a_frame_with_no_code_in_it_is_an_ordinary_empty_answer() {
        use image::{GrayImage, Luma};
        let blank = GrayImage::from_pixel(120, 120, Luma([255u8]));
        let mut png = std::io::Cursor::new(Vec::new());
        blank
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("encode png");

        assert!(
            decode_qr(&png.into_inner())
                .expect("a blank frame is not an error")
                .is_none(),
            "a camera pointed at a wall must answer 'nothing yet', never fail — \
             the scan loop sends frames continuously"
        );
    }

    #[test]
    fn something_that_is_not_an_image_is_an_error_not_a_shrug() {
        assert!(
            decode_qr(b"this is not a png").is_err(),
            "an unreadable frame is a caller mistake and must be visible as one"
        );
    }
}
