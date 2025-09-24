use super::{FshMessage, FshError, FshResult, FSH_MAGIC};
// Removed unused imports
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub struct FshCodec;

impl FshCodec {
    pub fn encode(message: &FshMessage) -> FshResult<Vec<u8>> {
        let mut buffer = Vec::new();

        // Write magic bytes
        buffer.extend_from_slice(FSH_MAGIC);

        // Serialize message
        let data = bincode::serialize(message)
            .map_err(|e| FshError::ProtocolError(format!("Serialization failed: {}", e)))?;

        // Write message length (4 bytes, big-endian)
        let length = data.len() as u32;
        buffer.extend_from_slice(&length.to_be_bytes());

        // Write message data
        buffer.extend_from_slice(&data);

        Ok(buffer)
    }

    pub fn decode(data: &[u8]) -> FshResult<FshMessage> {
        if data.len() < FSH_MAGIC.len() + 4 {
            return Err(FshError::ProtocolError("Insufficient data".to_string()));
        }

        // Check magic bytes
        if &data[..FSH_MAGIC.len()] != FSH_MAGIC {
            return Err(FshError::ProtocolError("Invalid magic bytes".to_string()));
        }

        // Read message length
        let length_bytes = &data[FSH_MAGIC.len()..FSH_MAGIC.len() + 4];
        let length = u32::from_be_bytes([
            length_bytes[0], length_bytes[1],
            length_bytes[2], length_bytes[3]
        ]) as usize;

        // Check if we have enough data
        let expected_total = FSH_MAGIC.len() + 4 + length;
        if data.len() < expected_total {
            return Err(FshError::ProtocolError("Incomplete message".to_string()));
        }

        // Deserialize message
        let message_data = &data[FSH_MAGIC.len() + 4..FSH_MAGIC.len() + 4 + length];
        bincode::deserialize(message_data)
            .map_err(|e| FshError::ProtocolError(format!("Deserialization failed: {}", e)))
    }

    pub async fn read_message<R>(reader: &mut R) -> FshResult<FshMessage>
    where
        R: AsyncRead + Unpin,
    {
        // Read magic bytes
        let mut magic = vec![0u8; FSH_MAGIC.len()];
        reader.read_exact(&mut magic).await
            .map_err(|e| FshError::NetworkError(format!("Failed to read magic: {}", e)))?;

        if magic != FSH_MAGIC {
            return Err(FshError::ProtocolError("Invalid magic bytes".to_string()));
        }

        // Read message length
        let mut length_bytes = [0u8; 4];
        reader.read_exact(&mut length_bytes).await
            .map_err(|e| FshError::NetworkError(format!("Failed to read length: {}", e)))?;

        let length = u32::from_be_bytes(length_bytes) as usize;

        // Validate length (prevent DoS attacks)
        if length > 10 * 1024 * 1024 { // 10MB max
            return Err(FshError::ProtocolError("Message too large".to_string()));
        }

        // Read message data
        let mut data = vec![0u8; length];
        reader.read_exact(&mut data).await
            .map_err(|e| FshError::NetworkError(format!("Failed to read data: {}", e)))?;

        // Deserialize message
        bincode::deserialize(&data)
            .map_err(|e| FshError::ProtocolError(format!("Deserialization failed: {}", e)))
    }

    pub async fn write_message<W>(writer: &mut W, message: &FshMessage) -> FshResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        let encoded = Self::encode(message)?;
        writer.write_all(&encoded).await
            .map_err(|e| FshError::NetworkError(format!("Failed to write message: {}", e)))?;
        writer.flush().await
            .map_err(|e| FshError::NetworkError(format!("Failed to flush: {}", e)))?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct MessageBuffer {
    buffer: Vec<u8>,
    messages: Vec<FshMessage>,
}

impl MessageBuffer {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            messages: Vec::new(),
        }
    }

    pub fn add_data(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
        self.try_parse_messages();
    }

    pub fn take_messages(&mut self) -> Vec<FshMessage> {
        std::mem::take(&mut self.messages)
    }

    fn try_parse_messages(&mut self) {
        while self.buffer.len() >= FSH_MAGIC.len() + 4 {
            // Check magic bytes
            if &self.buffer[..FSH_MAGIC.len()] != FSH_MAGIC {
                // Skip one byte and try again
                self.buffer.drain(0..1);
                continue;
            }

            // Read message length
            let length_bytes = &self.buffer[FSH_MAGIC.len()..FSH_MAGIC.len() + 4];
            let length = u32::from_be_bytes([
                length_bytes[0], length_bytes[1],
                length_bytes[2], length_bytes[3]
            ]) as usize;

            // Check if we have the complete message
            let total_length = FSH_MAGIC.len() + 4 + length;
            if self.buffer.len() < total_length {
                break; // Wait for more data
            }

            // Try to parse the message
            match FshCodec::decode(&self.buffer[..total_length]) {
                Ok(message) => {
                    self.messages.push(message);
                    self.buffer.drain(0..total_length);
                }
                Err(_) => {
                    // Skip this message and try again
                    self.buffer.drain(0..FSH_MAGIC.len());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::message::*;

    #[test]
    fn test_codec_roundtrip() {
        let original = FshMessage::Ping;
        let encoded = FshCodec::encode(&original).unwrap();
        let decoded = FshCodec::decode(&encoded).unwrap();

        match (original, decoded) {
            (FshMessage::Ping, FshMessage::Ping) => {},
            _ => panic!("Messages don't match"),
        }
    }

    #[test]
    fn test_message_buffer() {
        let mut buffer = MessageBuffer::new();

        let msg1 = FshMessage::Ping;
        let msg2 = FshMessage::Pong;

        let encoded1 = FshCodec::encode(&msg1).unwrap();
        let encoded2 = FshCodec::encode(&msg2).unwrap();

        // Add partial data
        buffer.add_data(&encoded1[..5]);
        assert_eq!(buffer.take_messages().len(), 0);

        // Add rest of first message
        buffer.add_data(&encoded1[5..]);
        let messages = buffer.take_messages();
        assert_eq!(messages.len(), 1);

        // Add second message
        buffer.add_data(&encoded2);
        let messages = buffer.take_messages();
        assert_eq!(messages.len(), 1);
    }
}