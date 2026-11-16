use nexora_language::{LiteralDefinition, LiteralMode};
pub struct ModuleSpecifier;
impl ModuleSpecifier {
    pub fn decode(raw: &str, definition: &LiteralDefinition) -> Result<String, String> {
        if raw.len() > 1024 {
            return Err("reference literal exceeds 1024 bytes".into());
        }
        let value = match definition.mode {
            LiteralMode::Raw => raw.to_owned(),
            LiteralMode::Quoted => {
                let quote = raw.chars().next().ok_or("empty reference literal")?;
                if !definition.quotes.iter().any(|q| q.starts_with(quote))
                    || raw.len() < quote.len_utf8() * 2
                    || !raw.ends_with(quote)
                {
                    return Err("reference literal does not match configured quotes".into());
                }
                let escape = definition.escape.as_ref().and_then(|s| s.chars().next());
                let mut chars = raw[quote.len_utf8()..raw.len() - quote.len_utf8()].chars();
                let mut value = String::new();
                while let Some(ch) = chars.next() {
                    if Some(ch) == escape {
                        let next = chars.next().ok_or("incomplete literal escape")?;
                        if (next == quote && definition.escaped_quote)
                            || (Some(next) == escape && definition.escaped_escape)
                        {
                            value.push(next);
                        } else if let Some(decoded) = definition.escapes.get(&next.to_string()) {
                            value.push_str(decoded);
                        } else {
                            return Err("unsupported configured literal escape".into());
                        }
                    } else if ch == quote {
                        return Err("unescaped literal delimiter".into());
                    } else {
                        value.push(ch);
                    }
                    if value.len() > 1024 {
                        return Err("decoded reference literal too large".into());
                    }
                }
                value
            }
        };
        if value.is_empty() || value.chars().any(char::is_control) {
            return Err("empty or control character in reference".into());
        }
        Ok(value)
    }
}
