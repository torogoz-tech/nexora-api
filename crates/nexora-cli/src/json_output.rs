use std::io::Write;

pub(crate) fn write_json(value: &impl serde::Serialize) -> Result<(), Box<dyn std::error::Error>> {
    let stdout = std::io::stdout();
    let mut output = std::io::BufWriter::new(stdout.lock());
    serde_json::to_writer_pretty(&mut output, value)?;
    writeln!(output)?;
    output.flush()?;
    Ok(())
}
