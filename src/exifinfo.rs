//! Minimal EXIF extraction for the metadata panel.

use std::path::Path;

#[derive(Default, Clone, Debug)]
pub struct ExifInfo {
    pub taken: Option<String>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub exposure: Option<String>,
}

fn clean(s: String) -> String {
    s.trim().trim_matches('"').trim().to_string()
}

pub fn read(path: &Path) -> ExifInfo {
    let mut out = ExifInfo::default();
    let Ok(file) = std::fs::File::open(path) else {
        return out;
    };
    let mut br = std::io::BufReader::new(file);
    let Ok(exif) = exif::Reader::new().read_from_container(&mut br) else {
        return out;
    };
    use exif::{In, Tag};
    let get = |t: Tag| {
        exif.get_field(t, In::PRIMARY)
            .map(|f| clean(f.display_value().to_string()))
    };
    out.taken = get(Tag::DateTimeOriginal).or_else(|| get(Tag::DateTime));
    let make = get(Tag::Make);
    let model = get(Tag::Model);
    out.camera = match (make, model) {
        (Some(mk), Some(md)) => {
            if md.to_lowercase().starts_with(&mk.to_lowercase()) {
                Some(md)
            } else {
                Some(format!("{mk} {md}"))
            }
        }
        (a, b) => a.or(b),
    };
    out.lens = get(Tag::LensModel);
    let mut parts: Vec<String> = Vec::new();
    if let Some(f) = exif.get_field(Tag::FNumber, In::PRIMARY)
        && let Some(v) = f
            .value
            .get_uint(0)
            .map(|x| x as f64)
            .or_else(|| match &f.value {
                exif::Value::Rational(r) if !r.is_empty() => Some(r[0].to_f64()),
                _ => None,
            })
    {
        parts.push(format!("f/{}", trim_f(v)));
    }
    if let Some(f) = exif.get_field(Tag::ExposureTime, In::PRIMARY)
        && let exif::Value::Rational(r) = &f.value
        && let Some(r0) = r.first()
        && r0.num > 0
        && r0.denom > 0
    {
        if r0.num < r0.denom {
            parts.push(format!(
                "1/{} s",
                (r0.denom as f64 / r0.num as f64).round() as u64
            ));
        } else {
            parts.push(format!("{} s", trim_f(r0.to_f64())));
        }
    }
    if let Some(f) = exif.get_field(Tag::PhotographicSensitivity, In::PRIMARY)
        && let Some(v) = f.value.get_uint(0)
    {
        parts.push(format!("ISO {v}"));
    }
    if let Some(f) = exif.get_field(Tag::FocalLength, In::PRIMARY)
        && let exif::Value::Rational(r) = &f.value
        && let Some(r0) = r.first()
        && r0.denom > 0
    {
        parts.push(format!("{} mm", trim_f(r0.to_f64())));
    }
    if !parts.is_empty() {
        out.exposure = Some(parts.join(" · "));
    }
    out
}

fn trim_f(v: f64) -> String {
    let s = format!("{v:.1}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
