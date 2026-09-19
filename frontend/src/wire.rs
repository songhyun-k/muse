use crate::generated::{API_VERSION, Event, MAX_MESSAGE_BYTES, Request};
use serde::de::DeserializeOwned;
use serde_json::Value;

pub fn decode_request(bytes: &[u8]) -> Result<Request, String> {
    let request: Request = decode(bytes)?;
    if request.version != API_VERSION || request.id == 0 {
        return Err("Unsupported protocol version or request ID".into());
    }
    Ok(request)
}

pub fn decode_event(bytes: &[u8]) -> Result<Event, String> {
    let event: Event = decode(bytes)?;
    if event.version != API_VERSION || event.id == Some(0) || event.sequence == 0 {
        return Err("Unsupported protocol version or event identity".into());
    }
    Ok(event)
}

pub fn encode_request(request: &Request) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(request).map_err(|error| error.to_string())?;
    // Re-decoding also rejects non-finite floats serialized as null by serde_json.
    decode_request(&bytes)?;
    Ok(bytes)
}

fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("Message exceeds 1 MiB".into());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    check_budget(&value, 0)?;
    // Decode from the original bytes so serde can reject duplicate object fields.
    serde_json::from_slice(bytes).map_err(|error| error.to_string())
}

fn check_budget(value: &Value, depth: usize) -> Result<(), String> {
    if depth > 24 {
        return Err("Message is too deeply nested".into());
    }
    match value {
        Value::String(text) if text.chars().count() > 8192 => {
            return Err("String is too long".into());
        }
        Value::Array(items) => {
            if items.len() > 32768 {
                return Err("Array is too large".into());
            }
            for item in items {
                check_budget(item, depth + 1)?;
            }
        }
        Value::Object(fields) => {
            for (key, item) in fields {
                if key.chars().count() > 8192 {
                    return Err("Object key is too long".into());
                }
                check_budget(item, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::{Command, SeekParams};

    #[test]
    fn shared_contract_fixtures() {
        let fixtures: Value =
            serde_json::from_str(include_str!("../../contract/fixtures.json")).unwrap();
        for (group, examples) in fixtures.as_object().unwrap() {
            for example in examples.as_array().unwrap() {
                let bytes = serde_json::to_vec(example).unwrap();
                match group.as_str() {
                    "validRequests" => {
                        let request = decode_request(&bytes).unwrap();
                        assert_eq!(
                            decode_request(&encode_request(&request).unwrap()).unwrap(),
                            request
                        );
                    }
                    "validEvents" => {
                        let event = decode_event(&bytes).unwrap();
                        let encoded = serde_json::to_vec(&event).unwrap();
                        assert_eq!(decode_event(&encoded).unwrap(), event);
                    }
                    "invalidEvents" => assert!(decode_event(&bytes).is_err(), "{example}"),
                    _ => assert!(decode_request(&bytes).is_err(), "{example}"),
                }
            }
        }
    }

    #[test]
    fn resource_limits_and_invalid_numbers() {
        assert!(decode_request(&vec![b' '; MAX_MESSAGE_BYTES + 1]).is_err());
        let long_text = Value::String("가".repeat(8193));
        assert!(check_budget(&long_text, 0).is_err());
        assert!(check_budget(&Value::Array(vec![Value::Null; 32769]), 0).is_err());
        assert!(check_budget(&Value::Null, 25).is_err());
        let request = Request {
            version: API_VERSION,
            id: 1,
            command: Command::Seek(SeekParams {
                seconds: f64::NAN,
                entry_id: "entry".into(),
            }),
        };
        assert!(encode_request(&request).is_err());
    }
}
