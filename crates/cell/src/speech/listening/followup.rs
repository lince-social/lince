use super::*;

pub(super) fn end(state: &mut State, detail: &str) {
    state.status.conversation = None;
    state.last_activity = None;
    state.pending = None;
    state.status.stage = ListeningStage::Waiting;
    state.status.detail = detail.into();
}

pub(super) async fn admit(
    shared: &Arc<Mutex<State>>,
    pipeline: &Pipeline,
    segment: &Segment,
    cancelled: &Arc<AtomicBool>,
    conversation: &Conversation,
    previous: Option<&EntryReceipt>,
    text: &str,
) -> Result<Admission, String> {
    let previous = previous.ok_or("The previous conversation receipt is unavailable.")?;
    let waiting = Instant::now();
    loop {
        if !current(&*shared.lock().await, segment, cancelled) {
            return Err("Listening stopped before the follow-up was checked.".into());
        }
        let receipt = pipeline.delivery.inspect(&previous.request_id).await?;
        if receipt
            .as_ref()
            .is_none_or(|receipt| !matches!(receipt.state.as_str(), "running" | "accepted"))
        {
            break;
        }
        if waiting.elapsed() >= Duration::from_secs(300) {
            return Err("The previous Fiote reply is still running.".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let dialogue = pipeline.delivery.context(previous).await?;
    let input = Input {
        request_id: format!("{}-{}-{}", segment.owner, segment.generation, segment.id),
        generation: segment.generation,
        text: text.into(),
        context: decision::Context {
            dialogue: Some(dialogue.clone()),
            ..Default::default()
        }
        .encode()?,
        candidates: Vec::new(),
        admission: true,
        routing: false,
    };
    let local_cancel = Arc::new(AtomicBool::new(false));
    let local = async {
        let output = pipeline
            .decision
            .submit(input.clone(), local_cancel.clone())?
            .await
            .map_err(|error| error.to_string())??;
        if output.request_id != input.request_id
            || output.generation != input.generation
            || output.model != pipeline.information.model
            || output.revision != pipeline.information.revision
        {
            return Ok(Some(Admission::Unclear));
        }
        Ok::<Option<Admission>, String>(output.admission.and_then(|choice| choice.value))
    };
    let classification = async {
        if pipeline.information.admission_validated && pipeline.decision.present() {
            match local.await {
                Ok(Some(admission)) => return Ok(admission),
                Ok(None) => {}
                Err(_) if cancelled.load(Ordering::Acquire) => {
                    return Err("Voice continuation was cancelled.".into());
                }
                Err(_) => {}
            }
        }
        pipeline
            .delivery
            .classify_followup(conversation, &dialogue, text)
            .await
    };
    tokio::pin!(classification);
    let deadline = tokio::time::sleep(Duration::from_secs(30));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            result = &mut classification => return result,
            _ = &mut deadline => { local_cancel.store(true, Ordering::Release); return Err("Voice continuation classification timed out.".into()); },
            _ = tokio::time::sleep(Duration::from_millis(25)) => {
                if cancelled.load(Ordering::Acquire) { local_cancel.store(true, Ordering::Release); return Err("Voice continuation was cancelled.".into()); }
            }
        }
    }
}
