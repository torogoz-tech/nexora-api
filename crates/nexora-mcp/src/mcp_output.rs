use crate::frame_buffer::FrameBuffer;
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};
pub(crate) struct McpOutput<W: Write + Send>(Arc<Mutex<W>>);
impl<W: Write + Send> Clone for McpOutput<W> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<W: Write + Send> McpOutput<W> {
    pub fn new(writer: W) -> Self {
        Self(Arc::new(Mutex::new(writer)))
    }
    pub fn send(&self, message: Value) -> io::Result<()> {
        let mut buffer = FrameBuffer(Vec::new());
        if serde_json::to_writer(&mut buffer, &message).is_err() {
            buffer.0.clear();
            let id = message.get("id").cloned().unwrap_or(Value::Null);
            let fallback = if message["result"]["content"].is_array() {
                json!({"jsonrpc":"2.0","id":id,"result":{"isError":true,"content":[{"type":"text","text":"Response exceeds 1 MiB; reduce limit, depth or line_count."}]}})
            } else {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32603,"message":"Response exceeds 1 MiB"}})
            };
            serde_json::to_writer(&mut buffer, &fallback).map_err(io::Error::other)?;
        }
        let mut writer = self
            .0
            .lock()
            .map_err(|_| io::Error::other("MCP output lock"))?;
        writer.write_all(&buffer.0)?;
        writer.write_all(b"\n")?;
        writer.flush()
    }
    pub fn error(&self, id: Value, code: i64, message: &str) -> io::Result<()> {
        self.send(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}))
    }
    pub fn tool(
        &self,
        id: Value,
        result: Result<Value, String>,
        structured: bool,
    ) -> io::Result<()> {
        let value = match result {
            Ok(value) => {
                let mut result = json!({"isError":false,"content":[{"type":"text","text":serde_json::to_string(&value).map_err(io::Error::other)?}]});
                if structured {
                    result["structuredContent"] = value;
                }
                result
            }
            Err(message) => json!({"isError":true,"content":[{"type":"text","text":message}]}),
        };
        self.send(json!({"jsonrpc":"2.0","id":id,"result":value}))
    }
}
