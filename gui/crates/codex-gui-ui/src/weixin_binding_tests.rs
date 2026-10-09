//! Tests for the in-GUI WeChat binding state machine and QR matrix.
#![allow(clippy::expect_used, clippy::panic)]

use super::*;
use pretty_assertions::assert_eq;

#[test]
fn qr_matrix_encodes_square_payload_with_quiet_zone() {
    let matrix = QrMatrix::encode("https://liteapp.weixin.qq.com/q/test-bot").expect("encodes");
    // Border ring must be entirely quiet so scanners lock onto the code.
    let last = matrix.size - 1;
    for i in 0..matrix.size {
        assert!(!matrix.is_dark(i, 0), "top edge dark at {i}");
        assert!(!matrix.is_dark(i, last), "bottom edge dark at {i}");
        assert!(!matrix.is_dark(0, i), "left edge dark at {i}");
        assert!(!matrix.is_dark(last, i), "right edge dark at {i}");
    }
    // The payload is non-trivial, so some module inside must be dark.
    assert!(matrix.cells.iter().any(|&dark| dark));
}

#[test]
fn qr_matrix_rejects_oversized_payload() {
    // Byte mode tops out far below 4 KB; nothing realistic to encode.
    assert!(QrMatrix::encode(&"x".repeat(4000)).is_none());
}

#[test]
fn events_drive_phase_transitions() {
    let mut binding = WeixinBinding::failed(String::new());
    binding.phase = BindingPhase::Starting;

    let url = "https://liteapp.weixin.qq.com/q/abc";
    apply_event(
        &mut binding,
        BindingEvent::QrReady(Arc::new(QrMatrix::encode(url).expect("matrix"))),
    );
    assert!(matches!(binding.phase, BindingPhase::QrReady { .. }));

    apply_event(&mut binding, BindingEvent::Scanned);
    assert!(matches!(binding.phase, BindingPhase::Scanned));

    apply_event(&mut binding, BindingEvent::NeedVerifyCode);
    assert!(matches!(binding.phase, BindingPhase::NeedVerifyCode));
    assert_eq!(binding.verify_input, "");

    binding.verify_input.push('4');
    apply_event(&mut binding, BindingEvent::Message("切换接入点".into()));
    assert!(matches!(binding.phase, BindingPhase::NeedVerifyCode));
    assert_eq!(binding.verify_input, "4");

    apply_event(
        &mut binding,
        BindingEvent::Finished(Ok("已连接微信（bot demo）".into())),
    );
    assert!(matches!(binding.phase, BindingPhase::Connected { .. }));
}

#[test]
fn refresh_replaces_matrix_and_note() {
    let mut binding = WeixinBinding::failed(String::new());
    binding.phase = BindingPhase::Starting;
    let first = Arc::new(QrMatrix::encode("https://x/1").expect("matrix"));
    apply_event(&mut binding, BindingEvent::QrReady(first.clone()));
    let second = Arc::new(QrMatrix::encode("https://x/2").expect("matrix"));
    apply_event(
        &mut binding,
        BindingEvent::QrRefreshed(2, second.clone(), "二维码已刷新（2）".into()),
    );
    match &binding.phase {
        BindingPhase::QrReady { matrix, note } => {
            assert!(Arc::ptr_eq(matrix, &second));
            assert_eq!(note, "二维码已刷新（2）");
        }
        other => panic!("expected QrReady, got {other:?}"),
    }
}

#[test]
fn failure_and_stop_are_cleared_on_finish() {
    let (stop_tx, _stop_rx) = tokio::sync::oneshot::channel::<()>();
    let mut binding = WeixinBinding::starting(stop_tx);
    assert!(binding.stop.is_some());

    apply_event(
        &mut binding,
        BindingEvent::Finished(Err("二维码多次失效".into())),
    );
    assert!(matches!(binding.phase, BindingPhase::Failed { .. }));
    assert!(binding.stop.is_none());
}

#[test]
fn submit_verify_code_trims_whitespace() {
    let mut binding = WeixinBinding::failed(String::new());
    binding.verify_input = " 1 2 3 ".into();
    binding.submit_verify_code();
    let code = binding
        .verify_code
        .lock()
        .map(|slot| slot.clone())
        .unwrap_or_default();
    assert_eq!(code, Some("1 2 3".to_string()));
}

#[test]
fn waiting_events_are_filtered() {
    assert!(map_login_event(LoginEvent::Waiting).is_none());
}
