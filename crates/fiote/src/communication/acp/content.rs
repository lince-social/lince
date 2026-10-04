use super::*;
use nucleus::message::MessagePart;

pub(super) fn from_agent(block: ContentBlock) -> Result<MessagePart, String> {
    let part = match block {
        ContentBlock::Text(text) => MessagePart::Text { text: text.text },
        ContentBlock::Image(image) => MessagePart::Attachment {
            name: "agent-image".into(),
            mime_type: image.mime_type,
            data: image.data,
        },
        ContentBlock::Audio(audio) => MessagePart::Attachment {
            name: "agent-audio".into(),
            mime_type: audio.mime_type,
            data: audio.data,
        },
        ContentBlock::ResourceLink(link) => MessagePart::Reference {
            name: link.name,
            uri: link.uri,
        },
        ContentBlock::Resource(resource) => match resource.resource {
            EmbeddedResourceResource::TextResourceContents(resource) => MessagePart::Text {
                text: resource.text,
            },
            EmbeddedResourceResource::BlobResourceContents(resource) => MessagePart::Attachment {
                name: "agent-file".into(),
                mime_type: resource
                    .mime_type
                    .unwrap_or_else(|| "application/octet-stream".into()),
                data: resource.blob,
            },
            _ => return Err("The agent returned an unsupported resource type.".into()),
        },
        _ => return Err("The agent returned an unsupported content type.".into()),
    };
    nucleus::message::validate(std::slice::from_ref(&part))?;
    Ok(part)
}

impl Connection {
    pub fn content(&self, parts: &[MessagePart]) -> Result<Vec<ContentBlock>, String> {
        nucleus::message::validate(parts)?;
        let capabilities = &self.info.agent_capabilities.prompt_capabilities;
        parts.iter().map(|part| Ok(match part {
            MessagePart::Question { question } => ContentBlock::Text(TextContent::new(question.text())),
            MessagePart::Steps { steps } => ContentBlock::Text(TextContent::new(nucleus::operation::steps_text(steps))),
            MessagePart::Text { text } => ContentBlock::Text(TextContent::new(text.clone())),
            MessagePart::Reference { name, uri } => ContentBlock::ResourceLink(ResourceLink::new(name.clone(), uri.clone())),
            MessagePart::Attachment { name, mime_type, data } => {
                if mime_type.starts_with("image/") {
                    if !capabilities.image { return Err("This agent does not accept image attachments. Remove the image or select a capable agent.".into()) }
                    ContentBlock::Image(ImageContent::new(data.clone(), mime_type.clone()))
                } else if mime_type.starts_with("audio/") {
                    if !capabilities.audio { return Err("This agent does not accept audio attachments. Use dictation for text, or select an audio-capable agent.".into()) }
                    ContentBlock::Audio(AudioContent::new(data.clone(), mime_type.clone()))
                } else {
                    if !capabilities.embedded_context { return Err("This agent does not accept embedded files. Send an accessible reference or select another agent.".into()) }
                    let uri = format!("lince-attachment:{}", name);
                    let resource = if mime_type.starts_with("text/") || mime_type == "application/json" {
                        let bytes = nucleus::message::decode(data)?;
                        let text = String::from_utf8(bytes).map_err(|_| "The text attachment is not UTF-8.")?;
                        EmbeddedResourceResource::TextResourceContents(TextResourceContents::new(text, uri).mime_type(mime_type.clone()))
                    } else {
                        EmbeddedResourceResource::BlobResourceContents(BlobResourceContents::new(data.clone(), uri).mime_type(mime_type.clone()))
                    };
                    ContentBlock::Resource(EmbeddedResource::new(resource))
                }
            }
        })).collect()
    }

    pub async fn prompt_content(
        &self,
        session: &str,
        text: String,
        parts: &[MessagePart],
        output: &dyn Output,
        mut stop: watch::Receiver<bool>,
    ) -> Result<String, String> {
        if text.len() > 512 * 1024 {
            return Err(
                "This conversation exceeds the agent text limit. Start a new thread.".into(),
            );
        }
        if parts.len() > 256
            || serde_json::to_vec(parts).map_err(failure)?.len() + text.len() > 8 * 1024 * 1024
        {
            return Err("The agent request exceeds its content limit. Start a new thread.".into());
        }
        let mut blocks = vec![ContentBlock::Text(TextContent::new(text))];
        for part in parts {
            blocks.extend(self.content(std::slice::from_ref(part))?);
        }
        let result = self.run_prompt(session, blocks, output, &mut stop).await;
        if result.is_err() {
            self.close();
        }
        result
    }
}
