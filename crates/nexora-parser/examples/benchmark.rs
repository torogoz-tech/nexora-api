use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use nexora_parser::Parser;
use std::{path::Path, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let pack = args
        .next()
        .ok_or("usage: benchmark PACK SOURCE [ITERATIONS]")?;
    let source = args.next().ok_or("source required")?;
    let iterations = args.next().map_or(Ok(20usize), |n| n.parse())?;
    if !(1..=1000).contains(&iterations) {
        return Err("iterations must be 1..1000".into());
    }
    let started = Instant::now();
    let language = LanguageDefinition::load(Path::new(&pack))?;
    let lexer = Lexer::new(&language.lexer)?;
    let parser = Parser::new(language.grammar);
    let compile_ms = started.elapsed().as_secs_f64() * 1000.0;
    let source = std::fs::read_to_string(source)?;
    let mut lex_ms = Vec::new();
    let mut parse_ms = Vec::new();
    let mut nodes = 0;
    let mut token_count = 0;
    for _ in 0..iterations {
        let started = Instant::now();
        let tokens = lexer.tokenize(&source)?;
        lex_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        let started = Instant::now();
        let tree = parser.parse(&tokens);
        parse_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        if !tree.diagnostics.is_empty() {
            return Err(format!("corpus contains {} diagnostics", tree.diagnostics.len()).into());
        }
        nodes = tree.nodes.len();
        token_count = tokens.len();
    }
    lex_ms.sort_by(f64::total_cmp);
    parse_ms.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"language":language.manifest.id.0,"bytes":source.len(),"tokens":token_count,"nodes":nodes,"iterations":iterations,"configuration_ms":compile_ms,"lex_median_ms":lex_ms[iterations/2],"parse_median_ms":parse_ms[iterations/2],"build":if cfg!(debug_assertions){"debug"}else{"release"}})
        )?
    );
    Ok(())
}
