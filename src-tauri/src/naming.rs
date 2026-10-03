use crate::models::{Assignment, Episode, Work};
use regex::Regex;
use std::{collections::BTreeMap, path::Path, sync::LazyLock};

static EPISODE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)第\s*([0-9]{1,3})\s*話|(?:^|[^a-z0-9])s[0-9]{1,2}e([0-9]{1,3})|(?:^|[^a-z0-9])ep?\s*([0-9]{1,3})").unwrap()
});
static RELEASE_NUMBER_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|\s[-–—]\s*)([0-9]{1,3})(?:$|\s|[-–—])|\[([0-9]{1,3})\]").unwrap()
});
pub fn infer_number(name: &str) -> Option<i64> {
    // Normalize common full-width filename characters without touching paths.
    let normalized: String = name
        .chars()
        .map(|c| match c {
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0).unwrap(),
            '\u{3000}' => ' ',
            _ => c,
        })
        .collect();
    let name = Path::new(&normalized).file_stem()?.to_str()?;
    let mut numbers = Vec::new();
    for pattern in [&*EPISODE_PATTERN, &*RELEASE_NUMBER_PATTERN] {
        for capture in pattern.captures_iter(name) {
            let number = (1..capture.len()).find_map(|i| capture.get(i)).unwrap();
            let tail = &name[number.end()..];
            let mut chars = tail.chars();
            let first = chars.next();
            // Reject partial numbers, fractional episodes and episode ranges.
            let mut trimmed_suffix = tail.trim_start().chars();
            if matches!(trimmed_suffix.next(), Some('-' | '–' | '—' | '~' | '〜'))
                && trimmed_suffix
                    .as_str()
                    .trim_start()
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
            {
                return None;
            }
            if first.is_some_and(|c| c.is_ascii_alphanumeric())
                || (matches!(first, Some('.' | '-' | '–' | '—' | '~' | '〜'))
                    && chars.next().is_some_and(|c| c.is_ascii_digit()))
            {
                return None;
            }
            if let Ok(value) = number.as_str().parse::<i64>() {
                numbers.push(value);
            }
        }
    }
    numbers.sort();
    numbers.dedup();
    if numbers.len() == 1 {
        numbers.first().copied()
    } else {
        None
    }
}
pub fn episode_label(number: Option<i64>, text: Option<&str>) -> String {
    if let Some(text) = text.filter(|s| !s.trim().is_empty()) {
        let normal = Regex::new(r"^(?:第)?\s*[0-9]+\s*(?:話)?$").unwrap();
        if !normal.is_match(text.trim()) {
            return text.trim().to_string();
        }
    }
    number
        .map(|n| format!("第{n:02}話"))
        .unwrap_or_else(|| text.unwrap_or_default().to_string())
}
pub fn resolved_values(
    work: &Work,
    episode: Option<&Episode>,
    a: &Assignment,
    original: &str,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let number = a.manual_number.or(episode.and_then(|e| e.number));
    let label = a
        .manual_label
        .clone()
        .unwrap_or_else(|| episode_label(number, episode.and_then(|e| e.number_text.as_deref())));
    let title = a
        .manual_title
        .clone()
        .unwrap_or_else(|| episode.and_then(|e| e.title.clone()).unwrap_or_default());
    let mut values = BTreeMap::from([
        ("work_title".into(), work.title.clone()),
        (
            "episode_number".into(),
            number.map(|n| n.to_string()).unwrap_or_default(),
        ),
        ("episode_label".into(), label.clone()),
        ("episode_title".into(), title.clone()),
        (
            "original_stem".into(),
            Path::new(original)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        ),
        ("annict_work_id".into(), work.annict_id.to_string()),
        (
            "annict_episode_id".into(),
            episode.map(|e| e.annict_id.to_string()).unwrap_or_default(),
        ),
    ]);
    values.values_mut().for_each(|v| *v = v.trim().to_string());
    let video_title = [label.trim(), title.trim()]
        .into_iter()
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut url = format!("https://annict.com/works/{}", work.annict_id);
    if let Some(e) = episode {
        url.push_str(&format!(
            "\nhttps://annict.com/works/{}/episodes/{}",
            work.annict_id, e.annict_id
        ));
    }
    let mut tags = BTreeMap::from([
        (
            "title".into(),
            if video_title.is_empty() {
                work.title.clone()
            } else {
                video_title
            },
        ),
        ("show".into(), work.title.clone()),
        ("comment".into(), url),
        ("episode_id".into(), label),
        ("description".into(), title),
        (
            "episode_sort".into(),
            number.map(|n| n.to_string()).unwrap_or_default(),
        ),
    ]);
    tags.values_mut().for_each(|v| *v = v.trim().to_string());
    (values, tags)
}

fn expand(segment: &str, values: &BTreeMap<String, String>) -> Result<(String, bool), String> {
    let mut output = String::new();
    let mut rest = segment;
    let mut complete = true;
    while let Some(start) = rest.find('{') {
        let literal = &rest[..start];
        if literal.contains('}') {
            return Err("テンプレートの括弧が対応していません".into());
        }
        output.push_str(literal);
        let end = rest[start..]
            .find('}')
            .ok_or("テンプレートの括弧が対応していません")?
            + start;
        let expression = &rest[start + 1..end];
        let (key, width) = expression
            .split_once(':')
            .map_or((expression, None), |(k, w)| (k, Some(w)));
        let value = values
            .get(key)
            .ok_or_else(|| format!("未知の変数: {key}"))?;
        complete &= !value.is_empty();
        if let Some(width) = width {
            if !["episode_number", "annict_work_id", "annict_episode_id"].contains(&key)
                || width.len() != 2
                || !width.starts_with('0')
            {
                return Err("桁数指定は数値変数の :02〜:09 のみ使用できます".into());
            }
            let width: usize = width.parse().map_err(|_| "桁数指定が不正です")?;
            if !(2..=9).contains(&width) {
                return Err("桁数指定は :02〜:09 のみ使用できます".into());
            }
            if !value.is_empty() {
                let n: i64 = value.parse().map_err(|_| "数値変数が不正です")?;
                output.push_str(&format!("{n:0width$}"));
            }
        } else {
            output.push_str(value);
        }
        rest = &rest[end + 1..];
    }
    if rest.contains('}') {
        return Err("テンプレートの括弧が対応していません".into());
    }
    output.push_str(rest);
    Ok((output, complete))
}
pub fn render_template(
    template: &str,
    values: &BTreeMap<String, String>,
) -> Result<String, String> {
    if template.contains(['/', '\\']) {
        return Err("テンプレートにフォルダ区切りは使用できません".into());
    }
    if template.chars().any(char::is_control) {
        return Err("テンプレートに制御文字は使用できません".into());
    }
    let mut output = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('[') {
        if rest[..start].contains(']') {
            return Err("省略区間の括弧が対応していません".into());
        }
        output.push_str(&expand(&rest[..start], values)?.0);
        let end = rest[start..]
            .find(']')
            .ok_or("省略区間の括弧が対応していません")?
            + start;
        let segment = &rest[start + 1..end];
        if segment.contains('[') {
            return Err("省略区間は入れ子にできません".into());
        }
        let (text, complete) = expand(segment, values)?;
        if complete {
            output.push_str(&text);
        }
        rest = &rest[end + 1..];
    }
    if rest.contains(']') {
        return Err("省略区間の括弧が対応していません".into());
    }
    output.push_str(&expand(rest, values)?.0);
    sanitize_name(&output)
}
pub fn sanitize_name(value: &str) -> Result<String, String> {
    let name: String = value
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut name = name.trim().trim_end_matches(['.', ' ']).to_string();
    if name.is_empty() {
        return Err("出力ファイル名が空です".into());
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        name.insert(0, '_');
    }
    if name.len() > 245 {
        return Err("ファイル名が長すぎます（拡張子を含め255バイト以内）".into());
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conservative_inference() {
        for (name, expected) in [
            ("作品 第03話 1080p.mp4", Some(3)),
            ("[group] anime S01E12.mkv", Some(12)),
            ("作品 EP02.mkv", Some(2)),
            ("作品 E04.mkv", Some(4)),
            ("2026 1080p 01.mkv", None),
            ("作品 EP01 第02話.mkv", None),
            ("anime1080p.mkv", None),
            ("animeEP01.mkv", None),
            ("anime EP01 EP02.mkv", None),
            ("anime E2026.mkv", None),
            ("anime EP01_1080p.mkv", Some(1)),
            ("anime EP12.5.mkv", None),
            ("anime EP01-02.mkv", None),
            (
                "[Mun] Yagate Kimi Ni Naru - 01 [Bdrip 1080P Hevc][Flac].mp4",
                Some(1),
            ),
            ("01 - わたしは星に届かない.mp4", Some(1)),
            ("作品 [03] [1080p].mkv", Some(3)),
            ("作品 - ０４ [1080p].mkv", Some(4)),
            ("作品 - 01-02.mkv", None),
            ("作品 - 01 - 02.mkv", None),
            ("作品 - 12.5.mkv", None),
            ("作品 - 1080p.mkv", None),
            ("作品 - 2026.mkv", None),
            ("作品 EP01 - 02.mkv", None),
            ("作品 [01] [02].mkv", None),
        ] {
            assert_eq!(infer_number(name), expected, "{name}");
        }
    }
    #[test]
    fn optional_and_padding() {
        let v = BTreeMap::from([
            ("work_title".into(), "作品".into()),
            ("episode_title".into(), String::new()),
            ("episode_number".into(), "2".into()),
        ]);
        assert_eq!(
            render_template("{work_title}[ - {episode_title}] - {episode_number:02}", &v).unwrap(),
            "作品 - 02"
        );
        for t in [
            "{unknown}",
            "../{work_title}",
            "[{work_title}",
            "{episode_number:20}",
            "{work_title:02}",
            "{work_title}}",
            "[[{work_title}]]",
        ] {
            assert!(render_template(t, &v).is_err(), "{t}");
        }
    }
    #[test]
    fn safe_names() {
        assert_eq!(sanitize_name("CON").unwrap(), "_CON");
        assert_eq!(sanitize_name("作品:題名? . ").unwrap(), "作品_題名_");
        assert!(sanitize_name("...").is_err());
        assert!(sanitize_name(&"あ".repeat(90)).is_err());
    }
    #[test]
    fn special_labels() {
        assert_eq!(episode_label(Some(1), Some("第1話")), "第01話");
        assert_eq!(episode_label(None, Some("番外編")), "番外編");
        assert_eq!(episode_label(None, None), "");
    }
}
