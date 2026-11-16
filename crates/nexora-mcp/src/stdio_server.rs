use crate::{
    McpConfiguration, mcp_output::McpOutput, mcp_service::McpService,
    protocol_session::ProtocolSession, request_registry::RequestRegistry,
};
use serde_json::Value;
use std::{
    io::{self, BufRead, Read, Write},
    sync::{Arc, Mutex, mpsc},
};

pub struct StdioServer;
impl StdioServer {
    pub fn serve<W: Write + Send + 'static>(
        config: McpConfiguration,
        mut input: impl BufRead,
        writer: W,
    ) -> io::Result<()> {
        let output = McpOutput::new(writer);
        let registry = Arc::new(Mutex::new(RequestRegistry::default()));
        let mut session = ProtocolSession::new(config.allow_index, registry.clone());
        session.allow_knowledge = config.knowledge_actor.is_some();
        let (send, receive) = mpsc::sync_channel::<(Value, String, Value, bool)>(8);
        let worker_output = output.clone();
        let worker = std::thread::spawn(move || -> io::Result<()> {
            let mut service = McpService::new(config);
            while let Ok((id, name, args, structured)) = receive.recv() {
                let key = serde_json::to_string(&id).map_err(io::Error::other)?;
                let cancelled = registry
                    .lock()
                    .map_err(|_| io::Error::other("MCP request lock"))?
                    .cancelled(&key);
                let result = if cancelled {
                    Err("cancelled".into())
                } else {
                    service.call(&name, &args)
                };
                let cancelled = registry
                    .lock()
                    .map_err(|_| io::Error::other("MCP request lock"))?
                    .complete(&key);
                if !cancelled {
                    worker_output.tool(id, result, structured)?;
                }
            }
            Ok(())
        });
        let read_result = (|| {
            loop {
                let mut line = Vec::new();
                let read = input
                    .by_ref()
                    .take(1_048_578)
                    .read_until(b'\n', &mut line)?;
                if read == 0 {
                    break;
                }
                if line.len() > 1_048_577
                    || (line.len() == 1_048_577 && line.last() != Some(&b'\n'))
                {
                    // Drain the rest of this frame in fixed buffers, preserving the next message.
                    if line.last() != Some(&b'\n') {
                        loop {
                            let buffer = input.fill_buf()?;
                            if buffer.is_empty() {
                                break;
                            }
                            let end = buffer.iter().position(|&b| b == b'\n');
                            let used = end.map_or(buffer.len(), |i| i + 1);
                            input.consume(used);
                            if end.is_some() {
                                break;
                            }
                        }
                    }
                    output.error(Value::Null, -32600, "Request exceeds 1 MiB")?;
                    continue;
                }
                if line.last() == Some(&b'\n') {
                    line.pop();
                }
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if line.len() > 1_048_576 {
                    output.error(Value::Null, -32600, "Request exceeds 1 MiB")?;
                    continue;
                }
                match serde_json::from_slice(&line) {
                    Ok(value) => session.handle(value, &output, &send)?,
                    Err(_) => output.error(Value::Null, -32700, "Parse error")?,
                }
            }
            Ok(())
        })();
        // Complete accepted frames on EOF; cancelled work in the queue is skipped.
        drop(send);
        let worker_result = worker
            .join()
            .map_err(|_| io::Error::other("MCP tool worker failed"))?;
        read_result.and(worker_result)
    }
}
