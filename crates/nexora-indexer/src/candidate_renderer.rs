pub struct CandidateRenderer;
impl CandidateRenderer {
    pub fn render(template: &str, module: &str, extension: &str) -> Result<String, String> {
        let mut result = String::new();
        let mut cursor = 0;
        while let Some(relative) = template[cursor..].find('{') {
            let start = cursor + relative;
            result.push_str(&template[cursor..start]);
            if template[start..].starts_with("{module}") {
                result.push_str(module);
                cursor = start + "{module}".len();
            } else if template[start..].starts_with("{extension}") {
                result.push_str(extension);
                cursor = start + "{extension}".len();
            } else {
                return Err("unknown candidate placeholder".into());
            }
            if result.len() > 4096 {
                return Err("candidate expansion budget".into());
            }
        }
        result.push_str(&template[cursor..]);
        if result.len() > 4096 {
            return Err("candidate expansion budget".into());
        }
        Ok(result)
    }
}
