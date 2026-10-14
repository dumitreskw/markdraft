//! Minimal front matter reader.
//!
//! MarkDraft does not ship a YAML parser; export only needs a handful of
//! top-level scalar keys, so this reads `key: value` lines and simple string
//! lists (`[a, b]` or `- item` lines below the key).

/// Document metadata taken from front matter.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FrontMatter {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub author: Option<String>,
    pub date: Option<String>,
}

/// Parses the raw front matter block (including its `---` delimiters).
pub fn parse(raw: &str) -> FrontMatter {
    let mut fm = FrontMatter::default();
    let lines: Vec<&str> = raw.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        if line.starts_with([' ', '\t', '-', '#']) || line.trim().is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let mut value = value.trim().to_owned();
        if value.is_empty() {
            // A block list follows: `- item` lines.
            let mut items = Vec::new();
            while i < lines.len() {
                let item = lines[i].trim_start();
                let Some(item) = item.strip_prefix("- ") else {
                    break;
                };
                items.push(unquote(item.trim()));
                i += 1;
            }
            value = items.join(", ");
        } else if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            value = inner
                .split(',')
                .map(|s| unquote(s.trim()))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
        } else {
            value = unquote(&value);
        }
        if value.is_empty() {
            continue;
        }
        let slot = match key.as_str() {
            "title" => &mut fm.title,
            "subtitle" => &mut fm.subtitle,
            "author" | "authors" => &mut fm.author,
            "date" => &mut fm.date,
            _ => continue,
        };
        if slot.is_none() {
            *slot = Some(value);
        }
    }
    fm
}

fn unquote(value: &str) -> String {
    let v = value.trim();
    let stripped = v
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')));
    match stripped {
        Some(s) => s.to_owned(),
        // Drop a trailing YAML comment from unquoted scalars.
        None => v.split(" #").next().unwrap_or(v).trim().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_scalars() {
        let fm = parse(
            "---\ntitle: \"Hello: World\"\nsubtitle: A tale # comment\nauthor: 'Ann'\ndate: 2024-05-01\nother: x\n---\n",
        );
        assert_eq!(fm.title.as_deref(), Some("Hello: World"));
        assert_eq!(fm.subtitle.as_deref(), Some("A tale"));
        assert_eq!(fm.author.as_deref(), Some("Ann"));
        assert_eq!(fm.date.as_deref(), Some("2024-05-01"));
    }

    #[test]
    fn reads_lists() {
        let fm = parse("---\nauthors: [Ann, \"Bob\"]\n---\n");
        assert_eq!(fm.author.as_deref(), Some("Ann, Bob"));
        let fm = parse("---\nauthor:\n  - Ann\n  - Bob\ntitle: T\n---\n");
        assert_eq!(fm.author.as_deref(), Some("Ann, Bob"));
        assert_eq!(fm.title.as_deref(), Some("T"));
    }

    #[test]
    fn ignores_nested_and_empty() {
        let fm = parse("---\nmeta:\n  title: nested\ntitle:\n---\n");
        assert_eq!(fm, FrontMatter::default());
    }
}
