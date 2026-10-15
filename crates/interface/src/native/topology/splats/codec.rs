use bevy::prelude::*;
use bevy_gaussian_splatting::{Gaussian3d, PlanarGaussian3d};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{self, BufRead, BufReader, Read, Write},
    path::Path,
};

pub const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_DECODED_BYTES: usize = 1024 * 1024 * 1024;
pub const MAX_SPLATS: usize = MAX_DECODED_BYTES / size_of::<Gaussian3d>();
pub const APPEARANCE_BYTES: usize = 48 * size_of::<f32>();

fn invalid(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

pub fn count(bytes: &[u8]) -> io::Result<usize> {
    let root = flexbuffers::Reader::get_root(bytes).map_err(invalid)?;
    let map = root.get_map().map_err(invalid)?;
    if map.len() != 4 {
        return Err(invalid("Expected a degree-3 FlexBuffers .gcloud"));
    }
    let mut count = None;
    for key in [
        "position_visibility",
        "spherical_harmonic",
        "rotation",
        "scale_opacity",
    ] {
        let length = map
            .index(key)
            .map_err(invalid)?
            .get_vector()
            .map_err(invalid)?
            .len();
        if length == 0 || length > MAX_SPLATS || count.is_some_and(|n| n != length) {
            return Err(invalid(
                "Cloud arrays must have matching nonzero lengths within the 1 GiB decoded-data budget",
            ));
        }
        count = Some(length);
    }
    Ok(count.unwrap())
}

pub fn decode(bytes: &[u8]) -> io::Result<(PlanarGaussian3d, super::super::assets::Bounds)> {
    count(bytes)?;
    let root = flexbuffers::Reader::get_root(bytes).map_err(invalid)?;
    let cloud = PlanarGaussian3d::deserialize(root).map_err(invalid)?;
    let bounds = validate(&cloud)?;
    Ok((cloud, bounds))
}

pub fn validate(cloud: &PlanarGaussian3d) -> io::Result<super::super::assets::Bounds> {
    let count = cloud.position_visibility.len();
    if count == 0
        || count > MAX_SPLATS
        || cloud.rotation.len() != count
        || cloud.scale_opacity.len() != count
        || cloud.spherical_harmonic.len() != count
    {
        return Err(invalid("Invalid Gaussian array lengths"));
    }
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for g in cloud.iter() {
        validate_gaussian(&g)?;
        if g.scale_opacity.opacity == 0.0 || g.position_visibility.visibility == 0.0 {
            continue;
        }
        let position = Vec3::from_array(g.position_visibility.position);
        let radius = Vec3::from_array(g.scale_opacity.scale).max_element() * 4.0;
        min = min.min(position - Vec3::splat(radius));
        max = max.max(position + Vec3::splat(radius));
    }
    if !min.is_finite() || !max.is_finite() || !(max - min).length().is_finite() {
        return Err(invalid("Cloud has no visible splats or has invalid bounds"));
    }
    Ok(super::super::assets::Bounds { min, max })
}

fn validate_gaussian(g: &Gaussian3d) -> io::Result<()> {
    let position = Vec3::from_array(g.position_visibility.position);
    let scale = Vec3::from_array(g.scale_opacity.scale);
    let rotation = Vec4::from_array(g.rotation.rotation);
    let visible = g.scale_opacity.opacity > 0.0 && g.position_visibility.visibility > 0.0;
    if !position.is_finite()
        || !scale.is_finite()
        || scale.min_element() < 0.0
        || !rotation.is_finite()
        || !(0.0..=1.0).contains(&g.scale_opacity.opacity)
        || !(0.0..=1.0).contains(&g.position_visibility.visibility)
        || g.spherical_harmonic
            .coefficients
            .iter()
            .any(|v| !v.is_finite())
        || (visible
            && ((rotation.length_squared() - 1.0).abs() > 0.01 || scale.min_element() <= 0.0))
    {
        return Err(invalid(
            "Cloud contains invalid positions, rotations, scales, opacity, or colors",
        ));
    }
    Ok(())
}

pub fn encode(cloud: &PlanarGaussian3d, mut output: impl Write) -> io::Result<usize> {
    validate(cloud)?;
    let mut serializer = flexbuffers::FlexbufferSerializer::new();
    cloud.serialize(&mut serializer).map_err(invalid)?;
    let bytes = serializer.view();
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(invalid(
            "Prepared .gcloud exceeds 1 GiB; reduce detail before converting",
        ));
    }
    output.write_all(bytes)?;
    Ok(bytes.len())
}

#[derive(Clone, Copy)]
enum Encoding {
    Ascii,
    Little,
    Big,
}

#[derive(Clone, Copy)]
enum Scalar {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

impl Scalar {
    fn parse(name: &str) -> io::Result<Self> {
        match name {
            "char" | "int8" => Ok(Self::I8),
            "uchar" | "uint8" => Ok(Self::U8),
            "short" | "int16" => Ok(Self::I16),
            "ushort" | "uint16" => Ok(Self::U16),
            "int" | "int32" => Ok(Self::I32),
            "uint" | "uint32" => Ok(Self::U32),
            "float" | "float32" => Ok(Self::F32),
            "double" | "float64" => Ok(Self::F64),
            _ => Err(invalid("Unsupported PLY scalar type")),
        }
    }

    fn read(self, reader: &mut impl Read, encoding: Encoding) -> io::Result<f32> {
        let size = match self {
            Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::F64 => 8,
            _ => 4,
        };
        let mut bytes = [0; 8];
        reader.read_exact(&mut bytes[..size])?;
        if matches!(encoding, Encoding::Big) {
            bytes[..size].reverse();
        }
        Ok(match self {
            Self::I8 => bytes[0] as i8 as f32,
            Self::U8 => bytes[0] as f32,
            Self::I16 => i16::from_le_bytes(bytes[..2].try_into().unwrap()) as f32,
            Self::U16 => u16::from_le_bytes(bytes[..2].try_into().unwrap()) as f32,
            Self::I32 => i32::from_le_bytes(bytes[..4].try_into().unwrap()) as f32,
            Self::U32 => u32::from_le_bytes(bytes[..4].try_into().unwrap()) as f32,
            Self::F32 => f32::from_le_bytes(bytes[..4].try_into().unwrap()),
            Self::F64 => f64::from_le_bytes(bytes) as f32,
        })
    }
}

fn line(reader: &mut impl BufRead, text: &mut String) -> io::Result<()> {
    text.clear();
    reader.take(16 * 1024).read_line(text)?;
    if text.is_empty() || !text.ends_with('\n') {
        return Err(invalid("Truncated or oversized PLY line"));
    }
    Ok(())
}

pub fn read_ply(reader: &mut impl BufRead) -> io::Result<PlanarGaussian3d> {
    let mut text = String::new();
    line(reader, &mut text)?;
    if text.trim() != "ply" {
        return Err(invalid("Expected a Gaussian PLY file"));
    }
    let mut encoding = None;
    let mut vertices = None;
    let mut properties = Vec::new();
    let mut header_bytes = text.len();
    loop {
        line(reader, &mut text)?;
        header_bytes += text.len();
        if header_bytes > 64 * 1024 {
            return Err(invalid("PLY header exceeds 64 KiB"));
        }
        let fields: Vec<_> = text.split_whitespace().collect();
        match fields.as_slice() {
            ["format", value, "1.0"] if encoding.is_none() => {
                encoding = Some(match *value {
                    "ascii" => Encoding::Ascii,
                    "binary_little_endian" => Encoding::Little,
                    "binary_big_endian" => Encoding::Big,
                    _ => return Err(invalid("Unsupported PLY encoding")),
                });
            }
            ["element", "vertex", value] if vertices.is_none() => {
                let count: usize = value.parse().map_err(invalid)?;
                if count == 0 || count > MAX_SPLATS {
                    return Err(invalid(
                        "PLY must contain splats within the 1 GiB decoded-data budget",
                    ));
                }
                vertices = Some(count);
            }
            ["property", scalar, name] if vertices.is_some() => {
                if properties.len() >= 128 || properties.iter().any(|(_, n)| n == name) {
                    return Err(invalid("Too many or repeated PLY properties"));
                }
                properties.push((Scalar::parse(scalar)?, name.to_string()));
            }
            ["comment" | "obj_info", ..] => {}
            ["end_header"] => break,
            _ => {
                return Err(invalid(
                    "Expected an uncompressed Gaussian PLY with scalar vertex properties only",
                ));
            }
        }
    }
    let encoding = encoding.ok_or_else(|| invalid("Missing PLY encoding"))?;
    let vertices = vertices.ok_or_else(|| invalid("Missing PLY vertices"))?;
    let names: BTreeSet<_> = properties.iter().map(|(_, n)| n.as_str()).collect();
    for required in [
        "x", "y", "z", "rot_0", "rot_1", "rot_2", "rot_3", "scale_0", "scale_1", "scale_2",
        "opacity", "f_dc_0", "f_dc_1", "f_dc_2",
    ] {
        if !names.contains(required) {
            return Err(invalid(format!(
                "Missing Gaussian PLY property: {required}"
            )));
        }
    }
    let rest = names.iter().filter(|n| n.starts_with("f_rest_")).count();
    if ![0, 9, 24, 45].contains(&rest)
        || (0..rest).any(|i| !names.contains(format!("f_rest_{i}").as_str()))
    {
        return Err(invalid(
            "Supported appearance degrees are 0, 1, 2, and 3; prepare higher degrees before import",
        ));
    }
    let mut cloud = PlanarGaussian3d::default();
    cloud
        .position_visibility
        .try_reserve_exact(vertices)
        .map_err(invalid)?;
    cloud
        .rotation
        .try_reserve_exact(vertices)
        .map_err(invalid)?;
    cloud
        .scale_opacity
        .try_reserve_exact(vertices)
        .map_err(invalid)?;
    cloud
        .spherical_harmonic
        .try_reserve_exact(vertices)
        .map_err(invalid)?;
    for _ in 0..vertices {
        let mut gaussian = Gaussian3d::default();
        let mut ascii = Vec::new();
        if matches!(encoding, Encoding::Ascii) {
            line(reader, &mut text)?;
            ascii = text.split_whitespace().collect();
            if ascii.len() != properties.len() {
                return Err(invalid("PLY row does not match its properties"));
            }
        }
        for (index, (scalar, name)) in properties.iter().enumerate() {
            let value = if matches!(encoding, Encoding::Ascii) {
                ascii[index].parse::<f32>().map_err(invalid)?
            } else {
                scalar.read(reader, encoding)?
            };
            if !value.is_finite() {
                return Err(invalid("PLY contains a non-finite value"));
            }
            match name.as_str() {
                "x" => gaussian.position_visibility.position[0] = value,
                "y" => gaussian.position_visibility.position[1] = value,
                "z" => gaussian.position_visibility.position[2] = value,
                "visibility" => gaussian.position_visibility.visibility = value,
                "opacity" => gaussian.scale_opacity.opacity = 1.0 / (1.0 + (-value).exp()),
                name if name.starts_with("rot_") => {
                    let i: usize = name[4..].parse().map_err(invalid)?;
                    *gaussian
                        .rotation
                        .rotation
                        .get_mut(i)
                        .ok_or_else(|| invalid("Invalid rotation property"))? = value;
                }
                name if name.starts_with("scale_") => {
                    let i: usize = name[6..].parse().map_err(invalid)?;
                    *gaussian
                        .scale_opacity
                        .scale
                        .get_mut(i)
                        .ok_or_else(|| invalid("Invalid scale property"))? = value.exp();
                }
                name if name.starts_with("f_dc_") => {
                    let i: usize = name[5..].parse().map_err(invalid)?;
                    if i > 2 {
                        return Err(invalid("Invalid base color property"));
                    }
                    gaussian.spherical_harmonic.coefficients[i] = value;
                }
                name if name.starts_with("f_rest_") => {
                    let i: usize = name[7..].parse().map_err(invalid)?;
                    let channel = i / (rest / 3);
                    let coefficient = 1 + i % (rest / 3);
                    gaussian.spherical_harmonic.coefficients[coefficient * 3 + channel] = value;
                }
                _ => {}
            }
        }
        let rotation = Vec4::from_array(gaussian.rotation.rotation)
            .try_normalize()
            .ok_or_else(|| invalid("PLY contains a zero rotation"))?;
        gaussian.rotation.rotation = rotation.to_array();
        validate_gaussian(&gaussian)?;
        cloud.position_visibility.push(gaussian.position_visibility);
        cloud.rotation.push(gaussian.rotation);
        cloud.scale_opacity.push(gaussian.scale_opacity);
        cloud.spherical_harmonic.push(gaussian.spherical_harmonic);
    }
    let mut remaining = [0; 4096];
    loop {
        let count = reader.read(&mut remaining)?;
        if count == 0 {
            break;
        }
        if !matches!(encoding, Encoding::Ascii)
            || remaining[..count]
                .iter()
                .any(|byte| !byte.is_ascii_whitespace())
        {
            return Err(invalid(
                "PLY contains unexpected data after its declared vertices",
            ));
        }
    }
    validate(&cloud)?;
    Ok(cloud)
}

pub fn convert_ply(source: &Path, destination: &Path) -> io::Result<(usize, usize)> {
    if source.canonicalize().ok() == destination.canonicalize().ok() || destination.exists() {
        return Err(invalid(
            "Choose a new output path; existing files are not overwritten",
        ));
    }
    let file = std::fs::File::open(source)?;
    if file.metadata()?.len() > MAX_FILE_BYTES {
        return Err(invalid("Source PLY exceeds 1 GiB"));
    }
    let cloud = read_ply(&mut BufReader::new(file.take(MAX_FILE_BYTES + 1)))?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut output = tempfile::NamedTempFile::new_in(parent)?;
    let bytes = encode(&cloud, &mut output)?;
    output.as_file().sync_all()?;
    output.persist_noclobber(destination).map_err(|e| e.error)?;
    Ok((cloud.position_visibility.len(), bytes))
}
