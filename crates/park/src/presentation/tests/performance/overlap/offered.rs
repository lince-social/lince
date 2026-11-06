use super::*;

pub(super) struct Offer {
    number: usize,
    planned: Instant,
    sent: Instant,
}

pub(super) enum Signal {
    Offered(Offer),
    Rendered,
}

fn trial(
    directory: &std::path::Path,
    mode: &str,
    workload: &str,
    count: usize,
    period: Duration,
    runtime: &tokio::runtime::Runtime,
    secondary_reference: &mut Option<u64>,
) -> serde_json::Value {
    assert!(count <= 24);
    let Scene {
        mut canvas,
        owner,
        editor,
        original,
        item,
    } = scene(directory, &format!("{workload}-{mode}"));
    assert_eq!(canvas.app.sub_apps().sub_apps.len(), 1);
    let mut render = Some(canvas.app.remove_sub_app(RenderApp).unwrap());
    assert!(canvas.app.sub_apps().sub_apps.is_empty());
    assert_send_resources(render.as_ref().unwrap());
    let origin = Instant::now();
    let before = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    let publisher = Publisher::new(origin);
    let (notify, incoming) = sync_channel(32);
    let worker = (mode != "serial").then(|| {
        RenderWorker::new(
            origin,
            runtime.handle().clone(),
            (mode == "overlap-worker-export")
                .then(|| (canvas.renderer.gpu.clone(), publisher.sender())),
            Some(notify.clone()),
        )
    });
    let producer = {
        let notify = notify.clone();
        std::thread::Builder::new()
            .name("park-offered-input-probe".into())
            .spawn(move || {
                for number in 0..count {
                    let planned = origin + Duration::from_millis(20) + period * number as u32;
                    if let Some(wait) = planned.checked_duration_since(Instant::now()) {
                        std::thread::sleep(wait);
                    }
                    if notify
                        .send(Signal::Offered(Offer {
                            number,
                            planned,
                            sent: Instant::now(),
                        }))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .unwrap()
    };
    let mut offers = Vec::new();
    let mut applied = 0;
    let mut prepared = None;
    let mut pending = None;
    let mut intervals = Vec::new();
    let mut max_queued_keys = 0;
    let mut event = None;
    let mut waits = 0;
    loop {
        for _ in 0..32 {
            let signal = event.take().or_else(|| incoming.try_recv().ok());
            let Some(signal) = signal else { break };
            match signal {
                Signal::Offered(offer) => {
                    assert_eq!(offer.number, offers.len());
                    offers.push(offer);
                }
                Signal::Rendered => {
                    assert!(render.is_none());
                    render = Some(receive(
                        worker.as_ref().unwrap(),
                        pending.take().unwrap(),
                        &canvas.renderer.gpu,
                        &publisher,
                        origin,
                        &mut intervals,
                    ));
                }
            }
        }
        max_queued_keys = max_queued_keys.max(offers.len() - applied);
        if render.is_some()
            && let Some(epoch) = prepared.take()
        {
            let epoch: Epoch = epoch;
            let number = epoch.number;
            let mut app = render.take().unwrap();
            app.extract(canvas.app.world_mut());
            canvas.app.world_mut().clear_trackers();
            if let Some(worker) = &worker {
                if mode == "overlap-worker-export" {
                    worker.submit(app, number, Some(epoch));
                    pending = Some(Pending::Exported(number));
                } else {
                    worker.submit(app, number, None);
                    pending = Some(Pending::Native(epoch));
                }
            } else {
                let start = origin.elapsed().as_secs_f64() * 1000.0;
                app.update();
                let end = origin.elapsed().as_secs_f64() * 1000.0;
                intervals.push((number, [start, end]));
                publisher
                    .sender()
                    .send(export(&canvas.renderer.gpu, &app, epoch, origin))
                    .unwrap();
                render = Some(app);
            }
            continue;
        }
        if prepared.is_none()
            && applied < offers.len()
            && (pending.is_none() || mode.starts_with("overlap"))
        {
            let mut epoch = prepare_inputs(
                &mut canvas,
                editor,
                owner,
                original,
                applied,
                offers.len() - 1,
                1.0,
                origin,
            );
            epoch.started = offers[applied].sent;
            applied = offers.len();
            prepared = Some(epoch);
            continue;
        }
        if applied == count && pending.is_none() && prepared.is_none() {
            assert_eq!(offers.len(), count);
            assert!(render.is_some());
            break;
        }
        waits += 1;
        event = Some(
            incoming
                .recv_timeout(Duration::from_secs(30))
                .expect("Timed input or render completion did not arrive"),
        );
    }
    producer.join().unwrap();
    drop(worker);
    drop(notify);
    let (frames, samples): (Vec<_>, Vec<_>) = publisher.finish().into_iter().unzip();
    let elapsed = origin.elapsed().as_secs_f64() * 1000.0;
    let after = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    let cpu = (after.tv_sec - before.tv_sec) as f64 * 1000.0
        + (after.tv_nsec - before.tv_nsec) as f64 / 1_000_000.0;
    assert!(waits > 0 && max_queued_keys <= count);
    assert_eq!(
        samples.last().unwrap()["epoch"].as_u64(),
        Some((count - 1) as u64)
    );
    assert_eq!(frames.len(), samples.len());
    let pixels = check_pixels(
        &frames,
        &samples,
        item,
        original,
        1.0,
        mode,
        secondary_reference,
    );
    let mut expected = 0;
    for sample in &samples {
        assert_eq!(sample["first"].as_u64().unwrap() as usize, expected);
        expected = sample["epoch"].as_u64().unwrap() as usize + 1;
    }
    assert_eq!(expected, count);
    let inputs = offers
        .iter()
        .map(|offer| {
            let sample = samples
                .iter()
                .find(|sample| sample["epoch"].as_u64().unwrap() as usize >= offer.number)
                .unwrap();
            let sent = offer.sent.duration_since(origin).as_secs_f64() * 1000.0;
            let planned = offer.planned.duration_since(origin).as_secs_f64() * 1000.0;
            let published = sample["published_ms"].as_f64().unwrap();
            assert!(sample["first"].as_u64().unwrap() as usize <= offer.number);
            assert!(published >= sent && sent >= planned);
            serde_json::json!({
                "input":offer.number,
                "planned_ms":planned,
                "offered_ms":sent,
                "producer_jitter_ms":sent-planned,
                "first_frame":sample["epoch"],
                "queue_to_main_ms":sample["main_ms"][0].as_f64().unwrap()-sent,
                "offered_to_publication_ms":published-sent,
                "planned_to_publication_ms":published-planned,
            })
        })
        .collect::<Vec<_>>();
    let overlap = samples
        .windows(2)
        .map(|pair| {
            let n = pair[0]["epoch"].as_u64().unwrap() as usize;
            let render = intervals.iter().find(|(epoch, _)| *epoch == n).unwrap().1;
            let main = pair[1]["main_ms"].as_array().unwrap();
            (render[1].min(main[1].as_f64().unwrap()) - render[0].max(main[0].as_f64().unwrap()))
                .max(0.0)
        })
        .sum::<f64>();
    if !mode.starts_with("overlap") {
        assert_eq!(overlap, 0.0);
    } else if workload == "sustained" {
        assert!(
            overlap > 0.0,
            "The sustained stream did not exercise overlap"
        );
    }
    let result = serde_json::json!({
        "mode":mode,"workload":workload,"period_ms":period.as_secs_f64()*1000.0,
        "input_count":count,"movement_px_per_key":1.0,"frames":frames.len(),"wall_ms":elapsed,"process_cpu_ms":cpu,
        "event_waits":waits,"max_queued_keys":max_queued_keys,"overlapped_wall_ms":overlap,
        "samples":samples,"render_intervals":intervals,"inputs":inputs,"pixels":pixels,
    });
    canvas.app.insert_sub_app(RenderApp, render.take().unwrap());
    canvas.finish().unwrap();
    result
}

#[test]
#[ignore = "private timed input native main/render replay; hardware Vulkan/EGL; no concurrent compilation"]
fn identical_offered_inputs_keep_order_pixels_and_bounded_event_driven_publication() {
    let checks = native::laboratory::run_interface_checks().unwrap();
    assert!(
        checks[0]
            .name
            .ends_with("::protein_area_filter_visible_and_selectable")
    );
    assert!(
        checks.iter().all(|result| result.error.is_none()),
        "{checks:?}"
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let path = std::path::PathBuf::from(
        std::env::var_os("LINCE_PARK_TEST_ARTIFACTS").expect("Set private artifacts"),
    );
    std::fs::create_dir_all(&path).unwrap();
    let mut modes = [
        "serial",
        "threaded-serial",
        "overlap",
        "overlap-worker-export",
    ];
    if std::env::var_os("LINCE_PARK_OVERLAP_REVERSE").is_some() {
        modes.reverse();
    }
    let mut secondary_reference = None;
    let mut reports = Vec::new();
    for (workload, count, period) in [
        ("sparse", 8, Duration::from_millis(240)),
        ("sustained", 24, Duration::from_millis(40)),
    ] {
        for mode in modes {
            let report = trial(
                directory.path(),
                mode,
                workload,
                count,
                period,
                &runtime,
                &mut secondary_reference,
            );
            eprintln!(
                "native-offered: {}",
                serde_json::json!({"mode":report["mode"],"workload":report["workload"],"frames":report["frames"],"wall_ms":report["wall_ms"],"process_cpu_ms":report["process_cpu_ms"]})
            );
            std::fs::write(
                path.join(format!("{workload}-{mode}.json")),
                serde_json::to_vec_pretty(&report).unwrap(),
            )
            .unwrap();
            reports.push(report);
        }
    }
    let result = serde_json::json!({
        "scope":"Private debug native-only timed input replay on two simulated outputs. Same planned deadlines per workload and variant; actual producer jitter and actual offered-to-first-ready-publication ages recorded for every key. Bounded event queue and one pending main/render epoch; ready notifications serviced while otherwise idle, all queued key transitions applied in order. The common consumer sees worker-owned export independently of busy main. Native main input batching is real; external clients, local persistence, physical presentation and production pipeline ownership are not covered. Readback occurs after timing; no shipping responsiveness or FPS acceptance claim.",
        "reports":reports,
    });
    std::fs::write(
        path.join("native-offered-inputs.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
}
