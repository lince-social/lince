use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;

use crate::error::EngineError;

const INVITE_PREFIX: &str = "lince1";

const SEP: char = '|';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingInvite {
    pub node_id: String,
    pub root_key: Option<String>,
    pub label: Option<String>,
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

const MAILBOX_PREFIX: &str = "lincemail1";

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

const ENROLMENT_PREFIX: &str = "lincecell1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrolmentInvite {
    pub node_id: String,
    pub organ_uid: String,
    pub root_key: String,
    pub token: String,
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

pub fn decode_qr(frame: &[u8]) -> Result<Option<String>, EngineError> {
    let image = image::load_from_memory(frame)
        .map_err(|error| EngineError::Consequence(format!("unreadable image: {error}")))?
        .to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(image);
    for grid in prepared.detect_grids() {
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
