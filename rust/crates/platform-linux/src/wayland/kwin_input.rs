// SPDX-License-Identifier: MIT
//! Guarded foreground input, not compositor-atomic target-bound input. Never
//! infer application success from focus checks. A failed post-check may mean
//! input reached another window; it must not be retried automatically.
use super::kwin_script::{self, Window};
use crate::interaction::RestoreMode;
use anyhow::{ensure, Context, Result};
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
pub(crate) struct Call {
    restore: Option<bool>,
    receipt: Mutex<serde_json::Value>,
    aims: Mutex<Vec<Window>>,
}
tokio::task_local! { static ASYNC_CALL: Arc<Call>; }
thread_local! {
    static CALL: RefCell<Option<Arc<Call>>> = const { RefCell::new(None) };
    static TARGET: RefCell<Option<Window>> = const { RefCell::new(None) };
}
pub(crate) fn current() -> Option<Arc<Call>> {
    CALL.with(|call| call.borrow().clone())
        .or_else(|| ASYNC_CALL.try_with(Arc::clone).ok())
}
pub(crate) fn bound<T>(call: Option<Arc<Call>>, body: impl FnOnce() -> T) -> T {
    struct Reset(Option<Arc<Call>>);
    impl Drop for Reset {
        fn drop(&mut self) {
            CALL.with(|c| *c.borrow_mut() = self.0.take());
        }
    }
    let _reset = Reset(CALL.with(|c| c.replace(call)));
    body()
}
fn note(value: serde_json::Value) {
    if let Some(call) = current() {
        *call.receipt.lock().unwrap() = value;
    }
}
pub(crate) fn aimed(window: &Window) {
    if let Some(call) = current() {
        call.aims.lock().unwrap().push(window.clone());
    }
}
pub(super) fn captured_target() -> Option<Window> {
    TARGET.with(|t| t.borrow().clone())
}

fn same_geometry(a: &Window, b: &Window) -> bool {
    (a.x, a.y, a.width, a.height, a.client_x, a.client_y, a.scale)
        == (b.x, b.y, b.width, b.height, b.client_x, b.client_y, b.scale)
}
#[derive(Debug, thiserror::Error)]
#[error("KWin target verification failed")]
pub(super) struct TargetVerificationFailed;

pub(super) fn check(target: &Window) -> Result<()> {
    (|| {
        crate::input::action::admit(|| ())?;
        let now = kwin_script::window(target.id, Some(target.pid))?;
        validate_target(target, &now)
    })()
    .context(TargetVerificationFailed)
}

fn validate_target(target: &Window, now: &Window) -> Result<()> {
    ensure!(
        target.id == now.id && target.uuid == now.uuid && target.pid == now.pid,
        "KWin target identity changed"
    );
    ensure!(
        now.active && now.on_screen && !now.minimized,
        "kwin_focus_changed: exact target is no longer active/visible"
    );
    ensure!(
        same_geometry(target, &now),
        "kwin_geometry_changed: target moved or changed scale; take a new window state"
    );
    Ok(())
}

/// Immediately bracket an actual libei dispatch, after readiness. A nested
/// invocation checks the outer target rather than taking a second focus lease.
pub(super) fn deliver<T>(body: impl FnOnce() -> Result<T>) -> Result<T> {
    let target = TARGET
        .with(|t| t.borrow().clone())
        .context("KWin input has no foreground target guard")?;
    deliver_checked(|| check(&target), body)
}

fn deliver_checked<T>(
    mut check: impl FnMut() -> Result<()>,
    body: impl FnOnce() -> Result<T>,
) -> Result<T> {
    if let Err(error) = check() {
        // Even a refused dispatch must latch the observed loss: focus may
        // return before cleanup, which must not then restore another window.
        note(
            serde_json::json!({"path":"kwin_script_libei", "verified":false,
            "effect":"unverifiable", "code":"kwin_target_unverified",
            "focus_after_delivery":false, "focus_lost_during_delivery":true,
            "focus_after":"unverified", "detail":format!("{error:#}"),
            "suggestion":"This dispatch sent no input. Inspect the target before retrying."}),
        );
        return Err(error);
    }
    let result = body();
    let after = check();
    if let Err(error) = after {
        note(
            serde_json::json!({"path":"kwin_script_libei", "verified":false, "effect":"unverifiable",
            "code":"kwin_delivery_unverified", "focus_after_delivery":false,
            "focus_lost_during_delivery":true,
            "focus_after":"unverified", "detail":format!("{error:#}"),
            "suggestion":"Input may have been delivered. Inspect the target before retrying."}),
        );
        return Err(error.context(
            "kwin_delivery_unverified: input may have been delivered; inspect before retrying",
        ));
    }
    if let Err(error) = &result {
        let already_failed =
            current().is_some_and(|call| call.receipt.lock().unwrap().get("code").is_some());
        if !already_failed {
            note(
                serde_json::json!({"path":"kwin_script_libei", "verified":false, "effect":"unverifiable",
            "code":"kwin_delivery_unverified", "focus_after_delivery":true,
            "focus_lost_during_delivery":error.is::<TargetVerificationFailed>(),
            "focus_after":"left_focused", "detail":format!("{error:#}"), "suggestion":"Input may be partial. Inspect the target before retrying."}),
            );
        }
    }
    result
}

fn restoration_allowed(receipt: &serde_json::Value) -> bool {
    receipt["focus_after_delivery"] != false && receipt["focus_lost_during_delivery"] != true
}

/// Screenshot coordinates are logical frame pixels, including decorations.
/// Scaling here would apply the output scale twice.
pub(super) fn map_point(window: &Window, x: i32, y: i32) -> Result<(i32, i32)> {
    ensure!(
        x >= 0 && y >= 0 && (x as u32) < window.width && (y as u32) < window.height,
        "point_outside_window: KWin window-relative point is outside the frame"
    );
    Ok((
        window.x.checked_add(x).context("coordinate overflow")?,
        window.y.checked_add(y).context("coordinate overflow")?,
    ))
}

fn restoration_result(result: &Result<()>, restored: bool) {
    if let Some(call) = current() {
        let mut receipt = call.receipt.lock().unwrap();
        if receipt.is_object() {
            match result {
                Ok(()) => {
                    receipt["focus_after"] = serde_json::json!(if restored {
                        "restored"
                    } else {
                        "no_previous_window"
                    })
                }
                Err(error) => {
                    receipt["focus_after"] = serde_json::json!("restore_failed");
                    receipt["restore_error"] = serde_json::json!(format!("{error:#}"));
                    // Preserve a delivery failure's original cause.
                    if receipt.get("code").is_none() {
                        receipt["code"] = serde_json::json!("kwin_focus_restore_failed");
                    }
                }
            }
        }
    }
}

/// Refuse a stale/outside/covered point instead of clicking an adjacent app.
/// KWin geometry and libei absolute regions both use global logical pixels.
pub(super) fn point(x: i32, y: i32) -> Result<()> {
    let target = TARGET
        .with(|t| t.borrow().clone())
        .context("KWin pointer has no target guard")?;
    let contains = |w: &Window| {
        x >= w.x
            && y >= w.y
            && i64::from(x) < i64::from(w.x) + i64::from(w.width)
            && i64::from(y) < i64::from(w.y) + i64::from(w.height)
    };
    ensure!(
        contains(&target),
        "point_outside_window: global logical point is outside the target frame"
    );
    kwin_script::verify_point(target.id, target.pid, x, y)?;
    Ok(())
}

pub(crate) fn foreground<T>(
    pid: u32,
    id: u64,
    mode: RestoreMode,
    body: impl FnOnce() -> Result<T>,
) -> Result<T> {
    if TARGET.with(|t| {
        t.borrow()
            .as_ref()
            .is_some_and(|w| w.id == id && w.pid == pid)
    }) {
        // The outer transaction owns focus and post-verification. Each actual
        // libei dispatch (and the EIS worker) verifies again after readiness;
        // do not add another pair of scripting round trips for every nesting.
        return body();
    }
    let restore = current()
        .and_then(|c| c.restore)
        .unwrap_or(mode == RestoreMode::RestorePrevious);
    crate::interaction::transaction(
        &mut (),
        |()| {
            let windows = kwin_script::list_windows()?;
            ensure!(
                windows.iter().any(|w| w.id == id && w.pid == pid),
                "KWin target UUID/PID disappeared"
            );
            Ok(Some(windows.into_iter().find(|w| w.active)))
        },
        |(), _| {
            kwin_script::activate(
                id,
                Some(pid),
                crate::interaction::operation_deadline(Duration::from_secs(3)),
            )
        },
        || {
            let target = kwin_script::window(id, Some(pid))?;
            if let Some(call) = current() {
                ensure!(call.aims.lock().unwrap().iter().filter(|w| w.id == id).all(|w| same_geometry(w, &target)),
                "kwin_geometry_changed: target moved since coordinates were resolved; no input sent");
            }
            crate::interaction::admit_lock(
                crate::interaction::policy::Scope::WindowInput,
                crate::interaction::facts::Fact::Known(target.active),
            )?;
            struct Reset;
            impl Drop for Reset {
                fn drop(&mut self) {
                    TARGET.with(|t| *t.borrow_mut() = None);
                }
            }
            TARGET.with(|t| *t.borrow_mut() = Some(target));
            let _reset = Reset;
            let _bound = super::bind_foreground_target(pid, id);
            let result = deliver(body);
            if result.is_ok() {
                note(
                    serde_json::json!({"path":"kwin_script_libei", "verified":false, "effect":"unverifiable",
                "delivery_mode":"foreground", "focus_before_delivery":true, "focus_after_delivery":true,
                "focus_after": if restore {"restore_requested"} else {"left_focused"},
                "evidence":[{"kind":"native_api_result", "detail":"KWin exact UUID/PID active before and after libei delivery; application effect requires readback. Focus can change between checks."}]}),
                );
            }
            result
        },
        |(), prior, cleanup| {
            if !restore {
                return Ok(());
            }
            // Do not steal focus back from a user who switched away during input.
            let result = (|| {
                ensure!(
                    !current()
                        .is_some_and(|call| !restoration_allowed(&call.receipt.lock().unwrap())),
                    "delivery focus was lost or unverifiable; prior focus was not restored"
                );
                let active = kwin_script::list_windows_until(cleanup)?
                    .into_iter()
                    .find(|w| w.active);
                ensure!(
                    active.as_ref().is_some_and(|w| w.id == id && w.pid == pid),
                    "focus changed after delivery; prior focus was not restored"
                );
                if let Some(prior) = prior.as_ref().filter(|w| w.id != id) {
                    kwin_script::activate(prior.id, Some(prior.pid), cleanup)?;
                }
                Ok(())
            })();
            restoration_result(&result, prior.is_some());
            result
        },
    )
}

fn attach_execution_record(
    result: &mut cua_driver_core::protocol::ToolResult,
    receipt: &serde_json::Value,
    requested: cua_driver_core::action_record::RequestedDelivery,
) {
    use cua_driver_core::action_record::{
        ActionEffect, ActionEvidence, ActionExecutionRecord, ActionTransport, ActualDelivery,
        EvidenceKind,
    };
    let mut record = ActionExecutionRecord::new(
        ActionEffect::Unverifiable,
        ActionTransport::LinuxLibei,
        requested,
    );
    record.actual_delivery = Some(if !restoration_allowed(receipt) {
        ActualDelivery::Unknown
    } else {
        ActualDelivery::Foreground
    });
    let focus = receipt
        .get("focus_after")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unverified");
    let mut detail = format!("KWin UUID/PID guarded shared libei delivery; focus_after={focus}. Focus checks do not confirm the application effect.");
    for field in ["detail", "restore_error"] {
        if let Some(cause) = receipt.get(field).and_then(serde_json::Value::as_str) {
            detail.push_str(&format!(" {field}: {cause}."));
        }
    }
    record.evidence.push(ActionEvidence {
        kind: EvidenceKind::NativeApiResult,
        detail: detail.clone(),
    });
    // The public outcome intentionally drops backend detail fields. Keep the
    // focus policy in its summary and attach the transport record explicitly;
    // the legacy path-name parser cannot infer our new adapter's transport.
    if let Some(cua_driver_core::protocol::Content::Text { text, .. }) = result.content.first_mut()
    {
        text.push(' ');
        text.push_str(&detail);
    }
    if let Some(existing) = result.action_record.as_mut() {
        // A preceding pixel-focus click can use libei while the actual edit
        // succeeds through AT-SPI. Keep that stronger transport/readback record.
        existing.evidence.extend(record.evidence);
    } else {
        result.action_record = Some(record);
    }
}

/// Adds per-call focus restoration and carries evidence from blocking workers
/// back to the agent-facing result without a process-global result slot.
pub(crate) struct Tool {
    inner: Box<dyn cua_driver_core::tool::Tool>,
    def: cua_driver_core::tool::ToolDef,
}
impl Tool {
    pub(crate) fn wrap(
        inner: Box<dyn cua_driver_core::tool::Tool>,
    ) -> Box<dyn cua_driver_core::tool::Tool> {
        let mut def = inner.def().clone();
        if def.input_schema["properties"]
            .get("delivery_mode")
            .is_none()
        {
            return inner;
        }
        def.input_schema["properties"]["restore_focus"] = serde_json::json!({"type":"boolean",
            "description":"KDE Wayland foreground input: restore the previous active window afterwards. Omit for the tool's usual focus policy; false leaves the target active."});
        Box::new(Self { inner, def })
    }
}
#[async_trait::async_trait]
impl cua_driver_core::tool::Tool for Tool {
    fn def(&self) -> &cua_driver_core::tool::ToolDef {
        &self.def
    }
    fn has_independent_input_lane(&self, args: &serde_json::Value) -> bool {
        self.inner.has_independent_input_lane(args)
    }
    async fn protected_resource_ownership(
        &self,
        adapter_id: &str,
        args: &serde_json::Value,
    ) -> cua_driver_core::tool::ProtectedResourceOwnership {
        self.inner
            .protected_resource_ownership(adapter_id, args)
            .await
    }
    async fn protected_resource_scope(
        &self,
        adapter_id: &str,
        args: &serde_json::Value,
    ) -> std::result::Result<
        Option<serde_json::Value>,
        cua_driver_core::tool::ProtectedResourceScopeError,
    > {
        self.inner.protected_resource_scope(adapter_id, args).await
    }
    async fn validate_protected_resource_scope(
        &self,
        adapter_id: &str,
        args: &serde_json::Value,
        approved: &serde_json::Value,
    ) -> std::result::Result<(), cua_driver_core::tool::ProtectedResourceScopeError> {
        self.inner
            .validate_protected_resource_scope(adapter_id, args, approved)
            .await
    }
    async fn invoke(&self, mut args: serde_json::Value) -> cua_driver_core::protocol::ToolResult {
        let restore = args
            .as_object_mut()
            .and_then(|a| a.remove("restore_focus"))
            .and_then(|v| v.as_bool());
        let call = Arc::new(Call {
            restore,
            ..Default::default()
        });
        let requested = if crate::input::delivery::DeliveryMode::from_args(&args).is_foreground() {
            cua_driver_core::action_record::RequestedDelivery::Foreground
        } else {
            cua_driver_core::action_record::RequestedDelivery::Background
        };
        let mut result = ASYNC_CALL
            .scope(call.clone(), self.inner.invoke(args))
            .await;
        let receipt = call.receipt.lock().unwrap().clone();
        if receipt.is_object() {
            attach_execution_record(&mut result, &receipt, requested);
        }
        if let Some(fields) = receipt.as_object() {
            let structured = result
                .structured_content
                .get_or_insert_with(|| serde_json::json!({}));
            for (key, value) in fields {
                structured[key] = value.clone();
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Window {
        serde_json::from_value(serde_json::json!({"uuid":"own", "pid":42,
            "title":"Task3", "app_name":"fixture", "x":-200, "y":40, "width":500,
            "height":300, "client_x":-200, "client_y":68, "active":true,
            "minimized":false, "on_screen":true, "stacking":1, "desktops":[],
            "output":"test", "scale":1.5}))
        .unwrap()
    }

    #[test]
    fn frame_coordinates_keep_negative_origins_and_do_not_apply_scale_twice() {
        let mut window = fixture();
        assert_eq!(map_point(&window, 0, 0).unwrap(), (-200, 40));
        assert_eq!(map_point(&window, 499, 299).unwrap(), (299, 339));
        for point in [(-1, 20), (20, -1), (500, 20), (20, 300)] {
            assert!(map_point(&window, point.0, point.1).is_err());
        }
        window.x = i32::MAX;
        assert!(map_point(&window, 1, 0).is_err());
    }

    #[test]
    fn partial_delivery_with_good_focus_preserves_its_cause_and_restoration() {
        let call = Arc::new(Call::default());
        bound(Some(call.clone()), || {
            let result: Result<()> =
                deliver_checked(|| Ok(()), || anyhow::bail!("EIS acknowledgement timed out"));
            assert!(result.is_err());
            restoration_result(&Ok(()), true);
        });
        let receipt = call.receipt.lock().unwrap();
        assert_eq!(receipt["focus_after_delivery"], true);
        assert_eq!(receipt["focus_after"], "restored");
        assert!(restoration_allowed(&receipt));
        assert!(receipt["detail"]
            .as_str()
            .unwrap()
            .contains("acknowledgement timed out"));
    }

    #[test]
    fn worker_focus_loss_remains_latched_even_when_the_postcheck_passes() {
        let call = Arc::new(Call::default());
        bound(Some(call.clone()), || {
            let result: Result<()> = deliver_checked(
                || Ok(()),
                || {
                    Err(anyhow::Error::new(TargetVerificationFailed)
                        .context("focus stolen in worker"))
                },
            );
            assert!(result.is_err());
        });
        let receipt = call.receipt.lock().unwrap();
        assert_eq!(receipt["focus_after_delivery"], true);
        assert_eq!(receipt["focus_lost_during_delivery"], true);
        assert!(!restoration_allowed(&receipt));
    }

    #[test]
    fn failed_precheck_disables_restoration_without_dispatching() {
        let call = Arc::new(Call::default());
        bound(Some(call.clone()), || {
            let result: Result<()> = deliver_checked(
                || anyhow::bail!("focus stolen before dispatch"),
                || panic!("no dispatch after failed precheck"),
            );
            assert!(result.is_err());
        });
        let receipt = call.receipt.lock().unwrap();
        assert_eq!(receipt["code"], "kwin_target_unverified");
        assert!(!restoration_allowed(&receipt));
    }

    #[test]
    fn restoration_failure_does_not_erase_verified_delivery_focus() {
        let call = Arc::new(Call::default());
        bound(Some(call.clone()), || {
            note(serde_json::json!({"focus_after_delivery":true}));
            restoration_result(&Err(anyhow::anyhow!("previous window closed")), true);
        });
        let receipt = call.receipt.lock().unwrap();
        assert_eq!(receipt["code"], "kwin_focus_restore_failed");
        assert_eq!(receipt["focus_after"], "restore_failed");
        assert_eq!(receipt["focus_after_delivery"], true);
        let mut result = cua_driver_core::protocol::ToolResult::error("restore failed");
        attach_execution_record(
            &mut result,
            &receipt,
            cua_driver_core::action_record::RequestedDelivery::Foreground,
        );
        assert_eq!(
            result.action_record.unwrap().actual_delivery,
            Some(cua_driver_core::action_record::ActualDelivery::Foreground)
        );
    }
    #[test]
    fn guarded_libei_result_has_a_publishable_execution_record() {
        use cua_driver_core::action_record::{ActionEffect, ActionTransport, RequestedDelivery};
        let mut result = cua_driver_core::protocol::ToolResult::text("Typed into the fixture.");
        attach_execution_record(
            &mut result,
            &serde_json::json!({"focus_after":"restored"}),
            RequestedDelivery::Foreground,
        );
        let record = result.action_record.unwrap();
        assert_eq!(record.transport, ActionTransport::LinuxLibei);
        assert_eq!(record.effect, ActionEffect::Unverifiable);
        record.validate().unwrap();
        record
            .public_result()
            .expect("the registry must publish this result instead of action_outcome_mismatch");
    }

    #[test]
    fn a_failed_precheck_sends_nothing_and_a_failed_postcheck_never_retries() {
        let sent = std::cell::Cell::new(0);
        let result = deliver_checked(
            || anyhow::bail!("focus stolen"),
            || {
                sent.set(sent.get() + 1);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(sent.get(), 0);
        let mut checks = 0;
        let result = deliver_checked(
            || {
                checks += 1;
                if checks == 2 {
                    anyhow::bail!("focus stolen");
                }
                Ok(())
            },
            || {
                sent.set(sent.get() + 1);
                Ok(())
            },
        );
        assert!(format!("{:#}", result.unwrap_err()).contains("kwin_delivery_unverified"));
        assert_eq!(sent.get(), 1);
    }

    #[test]
    fn nested_delivery_failures_keep_their_original_cause() {
        let call = Arc::new(Call::default());
        let result = bound(Some(call.clone()), || {
            deliver_checked(
                || Ok(()),
                || {
                    let mut reads = 0;
                    deliver_checked(
                        || {
                            reads += 1;
                            if reads > 1 {
                                anyhow::bail!("focus stolen");
                            }
                            Ok(())
                        },
                        || Ok(()),
                    )
                },
            )
        });
        assert!(result.is_err());
        assert!(call.receipt.lock().unwrap()["detail"]
            .as_str()
            .unwrap()
            .contains("focus stolen"));
    }

    #[test]
    fn identity_focus_visibility_geometry_and_scale_all_gate_delivery() {
        let expected = fixture();
        validate_target(&expected, &expected).unwrap();
        for mutate in [
            (|w: &mut Window| w.pid += 1) as fn(&mut Window),
            |w| w.id += 1,
            |w| w.uuid.push('x'),
            |w| w.active = false,
            |w| w.on_screen = false,
            |w| w.minimized = true,
            |w| w.x += 1,
            |w| w.width += 1,
            |w| w.scale = 2.,
            |w| w.client_y += 1,
        ] {
            let mut observed = expected.clone();
            mutate(&mut observed);
            assert!(validate_target(&expected, &observed).is_err());
        }
    }

    #[test]
    fn call_context_is_scoped_and_restored_after_nested_workers() {
        let outer = Arc::new(Call::default());
        bound(Some(outer.clone()), || {
            bound(None, || assert!(current().is_none()));
            assert!(Arc::ptr_eq(&current().unwrap(), &outer));
        });
        assert!(current().is_none());
    }
}
