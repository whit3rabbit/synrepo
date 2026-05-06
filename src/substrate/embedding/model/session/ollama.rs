use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::Result;

use super::super::OllamaModelResolution;

/// Local Ollama `/api/embed` embedding session.
#[derive(Debug)]
pub(super) struct OllamaEmbeddingSession {
    endpoint: String,
    model_name: String,
    dim: u16,
    normalize: bool,
    batch_size: usize,
    client: reqwest::blocking::Client,
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

impl OllamaEmbeddingSession {
    pub(super) fn new(res: &OllamaModelResolution) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| {
                crate::Error::Other(anyhow::anyhow!("Failed to create Ollama client: {e}"))
            })?;
        Ok(Self {
            endpoint: embed_url(&res.endpoint),
            model_name: res.model_name.clone(),
            dim: res.embedding_dim,
            normalize: res.normalize,
            batch_size: res.batch_size,
            client,
        })
    }

    pub(super) fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = Vec::with_capacity(texts.len());
        for batch in texts.chunks(self.batch_size) {
            out.extend(self.embed_batch(batch)?);
        }
        Ok(out)
    }

    pub(super) fn embedding_dim(&self) -> u16 {
        self.dim
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let response = self
            .client
            .post(&self.endpoint)
            .json(&EmbedRequest {
                model: &self.model_name,
                input: texts,
            })
            .send()
            .map_err(|e| {
                crate::Error::Other(anyhow::anyhow!("Ollama embed request failed: {e}"))
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(crate::Error::Other(anyhow::anyhow!(
                "Ollama embed request failed with status {status}: {body}"
            )));
        }

        let parsed: EmbedResponse = response.json().map_err(|e| {
            crate::Error::Other(anyhow::anyhow!(
                "Ollama embed response was invalid JSON: {e}"
            ))
        })?;
        if parsed.embeddings.len() != texts.len() {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "Ollama returned {} embeddings for {} inputs",
                parsed.embeddings.len(),
                texts.len()
            )));
        }

        parsed
            .embeddings
            .into_iter()
            .enumerate()
            .map(|(idx, mut vector)| {
                if vector.len() != self.dim as usize {
                    return Err(crate::Error::Other(anyhow::anyhow!(
                        "Ollama embedding {idx} has dimension {}, expected {}",
                        vector.len(),
                        self.dim
                    )));
                }
                if self.normalize {
                    normalize(&mut vector);
                }
                Ok(vector)
            })
            .collect()
    }
}

fn embed_url(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.ends_with("/api/embed") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/api/embed")
    }
}

fn normalize(vector: &mut [f32]) {
    let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-10 {
        for value in vector {
            *value /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;

    fn resolution(endpoint: String, dim: u16) -> OllamaModelResolution {
        OllamaModelResolution {
            endpoint,
            model_name: "all-minilm".to_string(),
            embedding_dim: dim,
            normalize: true,
            batch_size: 128,
        }
    }

    fn spawn_server(body: &'static str) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            let _ = tx.send(request);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{addr}"), rx)
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let n = stream.read(&mut chunk).unwrap();
            buffer.extend_from_slice(&chunk[..n]);
            if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let header_end = buffer.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
        let content_len = headers
            .lines()
            .find_map(|line| {
                line.strip_prefix("content-length:")
                    .or_else(|| line.strip_prefix("Content-Length:"))
                    .and_then(|value| value.trim().parse::<usize>().ok())
            })
            .unwrap_or(0);
        while buffer.len() < header_end + content_len {
            let n = stream.read(&mut chunk).unwrap();
            buffer.extend_from_slice(&chunk[..n]);
        }
        String::from_utf8_lossy(&buffer).to_string()
    }

    #[test]
    fn embeds_batch_and_normalizes_vectors() {
        let (endpoint, rx) = spawn_server(r#"{"embeddings":[[3.0,4.0],[0.0,5.0]]}"#);
        let session = OllamaEmbeddingSession::new(&resolution(endpoint, 2)).unwrap();
        let vectors = session.embed(&["a".to_string(), "b".to_string()]).unwrap();
        assert_eq!(vectors.len(), 2);
        assert!((vectors[0][0] - 0.6).abs() < 1e-6);
        assert!((vectors[0][1] - 0.8).abs() < 1e-6);
        assert!((vectors[1][1] - 1.0).abs() < 1e-6);

        let request = rx.recv().unwrap();
        assert!(request.starts_with("POST /api/embed "));
        assert!(request.contains(r#""model":"all-minilm""#));
        assert!(request.contains(r#""input":["a","b"]"#));
    }

    #[test]
    fn rejects_response_count_mismatch() {
        let (endpoint, _) = spawn_server(r#"{"embeddings":[[1.0,0.0]]}"#);
        let session = OllamaEmbeddingSession::new(&resolution(endpoint, 2)).unwrap();
        let err = session
            .embed(&["a".to_string(), "b".to_string()])
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("returned 1 embeddings for 2 inputs"));
    }

    #[test]
    fn rejects_dimension_mismatch() {
        let (endpoint, _) = spawn_server(r#"{"embeddings":[[1.0,0.0,0.0]]}"#);
        let session = OllamaEmbeddingSession::new(&resolution(endpoint, 2)).unwrap();
        let err = session.embed(&["a".to_string()]).unwrap_err();
        assert!(err.to_string().contains("has dimension 3, expected 2"));
    }
}
