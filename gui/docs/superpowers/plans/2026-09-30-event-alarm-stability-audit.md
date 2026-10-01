# Event Rules And Alarm Popup Stability Audit Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete an evidence-backed P0/P1/P2 stability audit of all 18 requested SmartGateWay event/alarm subsystems and produce sourced vendor comparison, defect closure, regression, and toggle-consistency deliverables.

**Architecture:** Audit in five isolated evidence tracks: runtime telemetry, C++ backend, Vue frontend, BM1688 hardware, and public benchmarking. Every observed failure becomes a four-part closure record (phenomenon, root cause, fix, validation). Read-only evidence collection is separated from fixes so code cannot be changed to fit an unsupported conclusion.

**Tech Stack:** C++17/Drogon, SQLite `linkage.db`, BM1688/CV186AH Sophon SDK, ZLMediaKit/FFmpeg, Vue 3/TypeScript/Vitest/Playwright, GoogleTest/CTest, Python standard-library audit tools.

**Spec:** User request dated 2026-09-30: end-to-end, deep-path, benchmarkable stability audit of event rules and alarm popups, covering 18 mandatory subsystems, 12 comparison dimensions, named vendors, four evidence rules, and five deliverables.

## Global Constraints

- Work in `/home/brewswang/workshop/SmartGateWay`; preserve its existing dirty work tree.
- Every finding cites code anchor, real-device test, or official public source.
- Public vendor cells use official documentation, standards, or primary release notes only.
- If no qualifying source exists, write “未检索到足以逐项支撑的公开资料” and mark the cell `unknown`.
- Every defect has 现象 → 根因 → 修复 → 验证; otherwise its status is `audit-gap`.
- Never deploy BM1688 artifacts to CV186AH or vice versa.
- Fault injection runs only against an isolated device/container, never production.
- Missing evidence is `not_verified`; it must never be reported as pass/fail.
- A result is closed only when its evidence artifact and regression command both exist.

## Artifact Contract

Create `reports/event_alarm_stability_audit_20260930/` with:

- `README.md`: commit hashes, hardware IDs, tool versions, dirty-path inventory, completion matrix, final verdict.
- `backend_findings.md`, `frontend_findings.md`, `hardware_findings.md`.
- `runtime_metrics.jsonl`, `fault_injection.jsonl`, `e2e_funnel.json`.
- `roi_identities.json`, `toggle_consistency.json`, `benchmark_sources.json`.
- `benchmark_matrix.md`, `defects.csv`, `regression_plan.md`.

JSON schemas:

```json
{"record_type":"metric","baseline_id":"<short-hash>-<timestamp>","case_id":"case-001","stage":"capture|decode|inference|rule|popup","ok":true,"value_ms":0,"details":{}}
```

```json
{"record_type":"fault","baseline_id":"<short-hash>-<timestamp>","case_id":"case-001","fault":"process_crash|dependency_down|fd_exhaust|disk_full|tpu_hang|stream_loss","observed_ms":0,"recovered":true,"details":{}}
```

`defects.csv` columns: `id,priority,subsystem,phenomenon,evidence_type,evidence_anchor,root_cause,fix,validation,status`.

---

### Task 1: Freeze Scope And Baseline

**Files:**
- Create: `reports/event_alarm_stability_audit_20260930/README.md`
- Create: `reports/event_alarm_stability_audit_20260930/runtime_metrics.jsonl`
- Create: `reports/event_alarm_stability_audit_20260930/fault_injection.jsonl`

**Interfaces:**
- Consumes: SmartGateWay Git state and target hardware.
- Produces: `baseline_id`, dirty-path inventory, toolchain inventory, and JSONL evidence sinks.

- [ ] Run and copy outputs into `README.md`:

```bash
cd /home/brewswang/workshop/SmartGateWay
git rev-parse HEAD
git status --porcelain=v1
uname -a
node --version
npm --version
cmake --version | head -1
```

- [ ] Record target BM1688/CV186AH model, firmware, Sophon SDK version, device ID, and isolated test endpoint.
- [ ] Write the 18-subsystem checklist with initial status `not_verified`.
- [ ] Initialize both JSONL files using the schemas above and the actual `baseline_id`.
- [ ] Re-run `git status --porcelain=v1`; verify the only delta is the new report directory.
- [ ] Commit only the new report scaffold as `audit: add event alarm stability baseline`.

### Task 2: Build Five-Stage E2E Harness

**Files:**
- Create: `scripts/audit/run_alarm_e2e_audit.py`
- Create: `tests/audit/test_run_alarm_e2e_audit.py`
- Create: `reports/event_alarm_stability_audit_20260930/cases.json`
- Modify: `reports/event_alarm_stability_audit_20260930/README.md`

**Interfaces:**
- Consumes: login API, rules API, alarm WebSocket/SSE, snapshot API, and SQLite.
- Produces: `run_alarm_e2e_audit.py --output-dir <dir>`; exit 0 only for completed cases or classified `no-fire`.

- [ ] Write failing tests:

```python
from run_alarm_e2e_audit import classify_no_fire, funnel_has_five_stages


def test_funnel_has_capture_decode_inference_rule_popup():
    event = {"capture_ts": 1, "decode_ts": 2, "inference_ts": 3, "rule_ts": 4, "popup_ts": 5}
    assert funnel_has_five_stages(event) is True


def test_no_fire_is_bucketed_not_hidden():
    assert classify_no_fire(False, "below_threshold") == "no-fire/below_threshold"
    assert classify_no_fire(False, None) == "no-fire/unclassified"
```

- [ ] Run `PYTHONPATH=tests/audit python3 -m pytest tests/audit/test_run_alarm_e2e_audit.py -q`; expect import failure.
- [ ] Implement standard-library CLI with `--api-base`, `--username`, `--password`, `--timeout-s`, `--case-id`, `--output-dir`, `--dry-run`.
- [ ] Generate cases directly from enabled DB rules and registered recorded fixtures; reject missing IDs instead of using placeholders.
- [ ] Include cases: popup under 5s, cooldown, suppression, four-channel aggregation, ROI private ownership, same ROI other rule no-fire, channel rebind, toggle-off, decoder fallback, VLM review.
- [ ] Record every event with capture, decode, inference, rule, and popup epoch-ms plus channel/rule/frame IDs.
- [ ] Classify every non-fire as `disabled`, `channel_mismatch`, `event_type_mismatch`, `time_window`, `roi_miss`, `below_threshold`, `duration_not_reached`, `cooldown`, `suppressed`, `merge_pending`, `subscription_blocked`, `dispatch_failed`, or `unclassified`.
- [ ] Run unit tests, `--dry-run`, then isolated real-device run.
- [ ] Commit harness, tests, case manifest, and evidence as `audit: add five-stage alarm e2e harness`.

### Task 3: Audit Watchdog, Heartbeat, Zombies, And Device Switching

**Files:**
- Read: `box-sdk/src/core/ServiceWatchdog.cpp`
- Read: `box-sdk/src/service/PipelineOrchestrator.cpp`
- Read: `box-sdk/src/service/DeviceService.cpp`
- Read: `box-sdk/src/iot/adapters/GB28181Adapter.cpp`
- Read: `scripts/tpu_auto_healthcheck.sh`
- Modify: `reports/event_alarm_stability_audit_20260930/backend_findings.md`

**Interfaces:**
- Produces: `WDG-001..NNN` findings with restart owner, interval, timeout, cleanup, lock, rollback, and code line.

- [ ] Extract crash restart, dependency heartbeat, child reap, online/offline debounce, and rollback paths from source.
- [ ] Inject four cases: backend SIGKILL, ZLMediaKit stop/start, zombie child, GB28181 offline→online.
- [ ] Record detection time, restart time, channel state, process count, fd count, RSS, and alarm continuity.
- [ ] Accept only when backend recovers within 10s, media dependencies within 15s, ten cycles leave zero zombies and zero fd growth, and each state transition emits exactly one final state.
- [ ] Write each failure as a defect row and commit `audit: verify watchdog and lifecycle recovery`.

### Task 4: Audit Rule Loading, Matching, Suppression, Aggregation, And Hot Reload

**Files:**
- Read: `box-sdk/src/service/alarm/LinkageEngine.cpp`
- Read: `box-sdk/src/service/alarm/AlarmDeduplicator.cpp`
- Read: `box-sdk/src/service/alarm/AlarmSuppressionEngine.cpp`
- Read: `box-sdk/src/service/alarm/AdaptiveThreshold.cpp`
- Read: `box-sdk/src/service/alarm/EventStore.cpp`
- Modify: `reports/event_alarm_stability_audit_20260930/backend_findings.md`

**Interfaces:**
- Produces: `RULE-001..NNN` findings for load, match, cooldown, suppression, aggregate, start/stop, and reload generation.

- [ ] Trace one rule ID through DB row, API payload, runtime copy, matching decision, cooldown key, suppression key, merge bucket, and persistence.
- [ ] Test immediate fire, threshold miss, active cooldown, active suppression, four-channel merge, and toggle-off under continuous events.
- [ ] Perform 50 enable/disable writes at 1Hz event load; record generation, stale decision, duplicate, lost event, queue depth, and DB/API/runtime state.
- [ ] Accept only when runtime generation advances, no state drifts after quiescence, no duplicate persists, and queue depth returns below its high-water mark.
- [ ] Commit findings and metrics as `audit: verify rule decision and reload stability`.

### Task 5: Audit Snapshot Capture, Decode, Persistence, URL, Cache, And Concurrency

**Files:**
- Read: `box-sdk/src/service/RtspFrameGrabber.cpp`
- Read: `box-sdk/src/service/alarm/VlmSnapshotAttacher.cpp`
- Read: `box-sdk/src/service/alarm/DetectedSnapshotRenderer.cpp`
- Read: `box-sdk/src/service/alarm/AlarmInfoFactory.cpp`
- Read: `box-sdk/src/pipeline/BufferPool.cpp`
- Read: `box-sdk/src/pipeline/DMABufBufferPool.cpp`
- Modify: `reports/event_alarm_stability_audit_20260930/backend_findings.md`

**Interfaces:**
- Produces: `SNAP-001..NNN` findings with frame ID, bytes hash, disk path, URL, decoder, ownership, and cache state.

- [ ] Trace one alarm snapshot from frame capture through decoder, renderer, encoder, disk write, URL, cache insert, and UI fetch.
- [ ] Hash source frame and fetched image; require identical bytes after documented lossless encoding and identical logical frame identity.
- [ ] Stress eight concurrent channels, forced hardware→software fallback, and 120% cache capacity.
- [ ] Record p50/p95, success ratio, disk bytes, hit/miss, eviction count, RSS, fd count, and stale URL count.
- [ ] Accept only when p95 snapshot-ready is under 2s, success is at least 99%, URLs never cross channels, cache returns to configured high-water mark in 30s, and RSS/fd return within 10% of baseline.
- [ ] Commit findings as `audit: verify snapshot persistence and cache safety`.

### Task 6: Audit GB28181 Preview, Reconnect, First Frame, And Adaptive Bitrate

**Files:**
- Read: `box-sdk/src/iot/adapters/GB28181Adapter.cpp`
- Read: `box-sdk/src/service/ZLMMediaKitAdapter.cpp`
- Read: `box-sdk/src/service/ZLMMediaSourceAdapter.cpp`
- Read: `box-sdk/src/service/StreamLifecycleManager.cpp`
- Read: `clients/web-admin/src/composables/useAdaptiveBitrate.ts`
- Read: `clients/web-admin/src/composables/useWebRTCPlayer.ts`
- Read: `clients/web-admin/src/composables/useStreamHealth.ts`
- Modify: `reports/event_alarm_stability_audit_20260930/frontend_findings.md`

**Interfaces:**
- Produces: `STREAM-001..NNN` findings for signal, media, player, retry, bitrate, and cleanup transitions.

- [ ] Capture state transitions: idle, registering, invited, playing, degraded, reconnecting, stopped.
- [ ] Test cold start, warm switch, 3s/10s/30s network loss, camera power cycle, and 100 open/close cycles.
- [ ] Record first-frame latency, stall duration, retry count, selected bitrate, WS/MediaSource/fd counts, RSS, and final playing state.
- [ ] Accept only when warm first frame is under 2s, cold first frame is under 5s, all temporary losses auto-recover without page reload, and 100 cycles leak zero resources.
- [ ] Commit findings as `audit: verify gb28181 preview recovery`.

### Task 7: Audit BM1688 VPU Decode And Failure Fallback

**Files:**
- Read: `box-sdk/src/service/ContinuousDecodeSource.cpp`
- Read: `box-sdk/src/service/StreamInferenceWorker.cpp`
- Read: `box-sdk/src/inference/SophonStreamIntegration.cpp`
- Read: `box-sdk/src/inference/SophonEngineAdapter.cpp`
- Read: `box-sdk/src/pipeline/plugins/CorePlugins.cpp`
- Modify: `reports/event_alarm_stability_audit_20260930/hardware_findings.md`

**Interfaces:**
- Produces: `VPU-001..NNN` findings for decoder selection, fault, fallback, recovery, and alarm continuity.

- [ ] On BM1688, log requested decoder, actual decoder, pixel format, resolution, fps, dropped frames, and error code.
- [ ] Inject malformed NAL, mid-stream resolution change, high-bitrate burst, and unsupported codec using recorded fixtures.
- [ ] Record VPU error, lost frames, fallback decoder, recovery time, CPU delta, RSS, fd count, and alarm continuity.
- [ ] If fallback is wrong, add `VpuDecodeFallbackTest.<FaultName>` to `box-sdk/tests/`; prove red before fix and green after.
- [ ] Commit findings as `audit: verify bm1688 vpu decode fallback`.

### Task 8: Audit TPU Models, Queues, Memory Reuse, And Thermal/Compute Limits

**Files:**
- Read: `box-sdk/src/inference/ModelManager.cpp`
- Read: `box-sdk/src/inference/BModelVerifier.cpp`
- Read: `box-sdk/src/inference/TPUResourceManager.cpp`
- Read: `box-sdk/src/inference/TPUMemoryBudget.cpp`
- Read: `box-sdk/src/service/InferenceResourceManager.cpp`
- Read: `box-sdk/src/service/InferenceScheduler.cpp`
- Read: `scripts/tpu_auto_healthcheck.sh`
- Modify: `reports/event_alarm_stability_audit_20260930/hardware_findings.md`

**Interfaces:**
- Produces: `TPU-001..NNN` findings for model load, queueing, memory reuse, throttling, deadlock, and rollback.

- [ ] Inventory every model: path, version, chip target, shapes, load latency, device memory, queue cap, timeout, retry, unload path.
- [ ] Run 90 minutes at production channel count, 2× queue saturation, model-load failure, TPU hang, and temperature-limit test.
- [ ] Sample every 5s: RSS, fd, device memory, queue depth, drops, inference p50/p95, temperature, utilization, watchdog state.
- [ ] Accept only when calls respect timeout, queues recover in 30s, failed load preserves or safely rejects the prior model, thermal state changes scheduling observably, and RSS/fd trend is flat within 10%.
- [ ] Commit findings as `audit: verify tpu resource and throttle safety`.

### Task 9: Audit Plugin Loading, Hot Swap, ABI, And Version Alignment

**Files:**
- Read: `box-sdk/src/plugin/PluginManager.cpp`
- Read: `box-sdk/src/plugin/PluginSymbolAnchor.cpp`
- Read: `box-sdk/src/plugin/AlgoPluginAdapter.cpp`
- Read: `box-sdk/src/plugin/algo/AlgoRegistry.cpp`
- Read: `box-sdk/src/pipeline/PluginFactory.cpp`
- Read: `box-sdk/src/pipeline/PluginSignature.cpp`
- Modify: `reports/event_alarm_stability_audit_20260930/backend_findings.md`

**Interfaces:**
- Produces: `PLUG-001..NNN` findings for ABI, symbols, versions, dlopen/dlclose, duplicate IDs, and rollback.

- [ ] Inventory every active plugin: hash, ABI, host version, expected/exported symbols, registry ID, alarm key, model dependency, and load result.
- [ ] Inject missing `.so`, missing symbol, stale ABI, duplicate ID, and replacement during active inference.
- [ ] Record host survival, old-instance drain, new-instance activation, failure count, fd count, and rollback result.
- [ ] Accept only when malformed plugins cannot kill the host, mismatch is rejected before dlopen, active work drains or times out, duplicate IDs fail atomically, and rollback restores the prior callable set.
- [ ] Commit findings as `audit: verify plugin abi and hot swap`.

### Task 10: Audit Snapshot/Streaming Routing, Intervals, Backoff, And Reroute

**Files:**
- Read: `box-sdk/src/service/InferenceScheduler.cpp`
- Read: `box-sdk/src/service/StreamInferenceWorker.cpp`
- Read: `box-sdk/src/service/RtspFrameGrabber.cpp`
- Read: `box-sdk/src/service/PipelineOrchestrator.cpp`
- Read: `box-sdk/src/service/AlgoDeploymentReconciler.cpp`
- Modify: `reports/event_alarm_stability_audit_20260930/backend_findings.md`

**Interfaces:**
- Produces: `SCHED-001..NNN` findings with selected mode, due interval, observed interval, retry, reroute, and drop reason.

- [ ] Build a truth table over enabled rule, online channel, snapshot mode, streaming mode, overload, decoder failure, and reconciler state.
- [ ] Run 20 channels for 15 minutes; remove source frames after 5 minutes and restore after 2 minutes.
- [ ] Classify each attempt as `selected`, `backoff`, `skipped`, or `dropped`; record expected and observed due deltas.
- [ ] Accept only when healthy interval error is under 20%, failures use bounded exponential backoff, recovery resets backoff, no channel starves others, and reroute leaves no orphan worker.
- [ ] Commit findings as `audit: verify inference scheduling recovery`.

### Task 11: Audit Plugin Main Path And Event Contract State Machine

**Files:**
- Read: `box-sdk/src/pipeline/plugins/AlarmCheckPlugin.cpp`
- Read: `box-sdk/src/pipeline/plugins/VideoUnderstandingPlugin.cpp`
- Read: `box-sdk/src/plugin/algo/AlgoInferenceHelper.cpp`
- Read: `box-sdk/src/service/alarm/AlarmInfoFactory.cpp`
- Read: `box-sdk/src/service/alarm/AlarmServiceImpl.cpp`
- Modify: `reports/event_alarm_stability_audit_20260930/backend_findings.md`

**Interfaces:**
- Produces: `ENGINE-001..NNN` findings proving each detection becomes one contract-valid alarm or classified no-fire.

- [ ] Extract required fields, timestamp units, coordinate space, confidence/severity ranges, canonical event key, lifecycle transitions, and bounded collection caps.
- [ ] Test duration pending, threshold pending, cooldown, fire, reset after inactivity, and duplicate detection.
- [ ] Require occur time, snapshot identity, canonical event key, channel ID, rule ID, and bounded related boxes on every emitted alarm.
- [ ] Accept only when state resets after inactivity and duplicate event contracts cannot emit twice.
- [ ] Commit findings as `audit: verify alarm contract state machine`.

### Task 12: Prove Private ROI Identity And Cross-Channel Isolation

**Files:**
- Read: `clients/web-admin/src/composables/useRoiCanvas.ts`
- Read: `clients/web-admin/src/composables/roiSchema.ts`
- Read: `clients/web-admin/src/composables/useAlarmShapes.ts`
- Read: `box-sdk/src/pipeline/RegionStore.cpp`
- Read: `box-sdk/src/core/RestApiHandlers.cpp`
- Read: `box-sdk/src/service/alarm/LinkageEngine.cpp`
- Create: `scripts/audit/check_roi_identity.py`
- Create: `tests/audit/test_check_roi_identity.py`
- Modify: `reports/event_alarm_stability_audit_20260930/roi_identities.json`

**Interfaces:**
- Produces: canonical SHA-256 ROI identity and equality verdicts for draw, save, inference, and annotation layers.

- [ ] Define one canonical serializer over `rule_id`, `region_id`, `channel_id`, geometry type, ordered geometry, integer `x10000`/`y10000`, and schema version:

```python
import hashlib
import json


def roi_identity(roi):
    canonical = {
        "channel_id": str(roi["channel_id"]),
        "geometry": roi["geometry"],
        "geometry_type": roi["geometry_type"],
        "region_id": str(roi["region_id"]),
        "rule_id": str(roi["rule_id"]),
        "schema_version": 1,
        "x10000": int(round(roi["x"] * 10000)),
        "y10000": int(round(roi["y"] * 10000)),
    }
    payload = json.dumps(canonical, ensure_ascii=False, separators=(",", ":"), sort_keys=True)
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()
```

- [ ] Add tests proving insignificant float draw noise preserves identity and changing rule ID changes identity.
- [ ] Enumerate every ROI from backend and API; emit frontend, storage, inference, and annotation hashes.
- [ ] Fail missing/mutable region ID, name-based lookup, cross-rule sharing, cross-channel reuse, or any hash mismatch.
- [ ] If defective, add `RoiOwnershipTest.PrivateByRuleAndChannel` and a Vitest ownership case; migrate storage and lookup to ID-based private ownership.
- [ ] Commit checker, tests, and evidence as `audit: enforce private roi identity by id`.

### Task 13: Audit Linkage Actions, No-Fire Buckets, And Subscription Degradation

**Files:**
- Read: `box-sdk/src/service/alarm/LinkageEngine.cpp`
- Read: `box-sdk/src/service/alarm/AlarmDispatcher.cpp`
- Read: `box-sdk/src/service/alarm/AlarmSuppressionEngine.cpp`
- Read: `box-sdk/src/service/cep/CEPEngine.cpp`
- Read: `box-sdk/src/service/playbook/PlaybookEngine.cpp`
- Read: `box-sdk/src/service/relay/CrossChannelRelay.cpp`
- Modify: `reports/event_alarm_stability_audit_20260930/backend_findings.md`

**Interfaces:**
- Produces: `LINK-001..NNN` action-chain findings and exhaustive no-fire classification.

- [ ] For every action type, record attempted, succeeded, failed, retried, degraded, subscription-skipped, gate-skipped, and rolled-back plus target ID, latency, and error.
- [ ] Inject slow first failure, fast second failure, and successful third action; verify isolation, bounded retry, no silent swallow, and no rollback of the independent success.
- [ ] Verify every attempted event maps to one exhaustive bucket from Task 2; mark `unclassified` as P0.
- [ ] Commit findings as `audit: verify action isolation and no-fire buckets`.

### Task 14: Enforce Annotation Parity In Popup, List, And Preview

**Files:**
- Read: `clients/web-admin/src/components/alarm/AlarmPopup.vue`
- Read: `clients/web-admin/src/components/alarm/AlarmCard.vue`
- Read: `clients/web-admin/src/components/alarm/AlarmSnapshot.vue`
- Read: `clients/web-admin/src/composables/useAlarmShapes.ts`
- Read: `clients/web-admin/src/utils/evidenceFrames.ts`
- Read: `box-sdk/src/service/alarm/DetectedSnapshotRenderer.cpp`
- Create: `tests/web-admin/e2e/alarm-annotation-consistency.spec.ts`
- Modify: `reports/event_alarm_stability_audit_20260930/frontend_findings.md`

**Interfaces:**
- Produces: normalized `related_box`, rule region, detection region, and face region parity evidence across three UI surfaces.

- [ ] Add Playwright test IDs: `alarm-popup-canvas`, `alarm-list-canvas`, `alarm-preview-canvas`, `related-box-normalized`, `rule-region-normalized`.
- [ ] Assert all surfaces use the same ordered normalized coordinates: related `[[0.1,0.2],[0.3,0.4]]`, rule `[[0.05,0.05],[0.8,0.7]]`.
- [ ] Assert detection, face, and region labels each render once and hidden arrays render nothing.
- [ ] Save full-page screenshot to `reports/event_alarm_stability_audit_20260930/annotation-consistency.png`.
- [ ] Run from `clients/web-admin`: `npx playwright test ../../tests/web-admin/e2e/alarm-annotation-consistency.spec.ts`.
- [ ] Fix only proven mismatches and commit `audit: enforce alarm annotation parity`.

### Task 15: Prove Rule Binding Portability By ID

**Files:**
- Read: `box-sdk/db/init_box.sql`
- Read: `box-sdk/src/service/alarm/LinkageEngine.cpp`
- Read: `box-sdk/src/core/RestApiHandlers.cpp`
- Read: `clients/web-admin/src/components/linkage/DeviceChannelPicker.vue`
- Read: `clients/web-admin/src/composables/useRuleChannelDisplay.ts`
- Read: `clients/web-admin/src/views/LinkageRuleView.vue`
- Modify: `reports/event_alarm_stability_audit_20260930/backend_findings.md`

**Interfaces:**
- Produces: `BIND-001..NNN` findings for one/many/all channel binding and ID-safe channel move.

- [ ] Clone one rule template into one, two, selected-subset, and all-channel bindings.
- [ ] Fire each bound and unbound channel; verify only bound channels trigger and time/spatial filters stay independent.
- [ ] Move binding from channel A to B while events continue; verify A stops, B starts, no duplicate occurs, and A's ROI is not reused for B.
- [ ] Compare create payload, DB JSON, individual GET, all-rules GET, and runtime copy; require string ID matching and reject name matching.
- [ ] Commit findings as `audit: verify rule channel binding portability`.

### Task 16: Audit Memory, FD, CPU, Disk, TPU Deadlock, And Rollback

**Files:**
- Create: `scripts/audit/run_stability_faultsuite.sh`
- Modify: `reports/event_alarm_stability_audit_20260930/fault_injection.jsonl`
- Modify: `reports/event_alarm_stability_audit_20260930/regression_plan.md`

**Interfaces:**
- Produces: `STAB-001..NNN` evidence for all six requested resource risks.

- [ ] Create an isolated-endpoint fault suite that samples timestamp, RSS, fd count, CPU, and disk percentage:

```bash
#!/usr/bin/env bash
set -euo pipefail
OUTPUT_DIR="${1:?usage: run_stability_faultsuite.sh <report-dir>}"
: "${SERVICE_PID:?SERVICE_PID must identify the isolated target process}"
: "${SC_CLK:?SC_CLK must be clock ticks per second}"
mkdir -p "$OUTPUT_DIR"
printf '{"ts_epoch_ms":%s,"rss_kb":%s,"fd_count":%s,"disk_used_pct":%s}\n' \
  "$(date +%s%3N)" \
  "$(awk '/VmRSS/ {print $2}' "/proc/$SERVICE_PID/status")" \
  "$(find "/proc/$SERVICE_PID/fd" -mindepth 1 -maxdepth 1 | wc -l)" \
  "$(df -P /data/shield | awk 'NR==2 {gsub("%","",$5); print $5}')"
```

- [ ] Run long-window RSS growth, repeated open/close fd stress, 1Hz alarm burst, disk 95%→99%→cleanup on scratch mount, TPU hang, and broken package rollback.
- [ ] Require nonpositive RSS/fd slope, CPU p95 return within 60s, safe disk rejection before exhaustion, bounded TPU recovery, and healthy rollback with prior artifact hash.
- [ ] Commit suite and evidence as `audit: run resource and rollback faultsuite`.

### Task 17: Enforce UI/API/Runtime/DB Toggle Consistency

**Files:**
- Read: `scripts/ci/check_rule_toggle_consistency.py`
- Read: `clients/web-admin/src/views/LinkageRuleView.vue`
- Read: `clients/web-admin/src/api/linkage.ts`
- Read: `box-sdk/src/core/RestApiHandlers.cpp`
- Read: `box-sdk/src/service/alarm/LinkageEngine.cpp`
- Modify: `scripts/ci/check_rule_toggle_consistency.py`
- Create: `tests/audit/test_check_rule_toggle_consistency.py`
- Modify: `reports/event_alarm_stability_audit_20260930/toggle_consistency.json`

**Interfaces:**
- Produces: four-source checker output; exit 0 only when UI, API, runtime, and DB all agree for every rule ID.

- [ ] Add strict helpers:

```python
def normalize_rule_state(enabled, rule_id):
    if rule_id is None or str(rule_id) == "":
        raise ValueError("rule_id is required")
    if enabled is None:
        value = None
    elif isinstance(enabled, bool):
        value = enabled
    else:
        text = str(enabled).strip().lower()
        value = text in {"1", "true", "t", "yes"}
        if text not in {"0", "1", "true", "false", "t", "f", "yes", "no"}:
            raise ValueError(f"invalid enabled value: {enabled}")
    return {"rule_id": str(rule_id), "enabled": value}
```

- [ ] Emit each rule as `{"rule_id":"17","ui":true,"api":true,"runtime":true,"db":true,"consistent":true,"evidence":{}}`.
- [ ] Missing runtime is `runtime:null`, `consistent:false`, and `evidence.runtime_error`; never infer it.
- [ ] Test DB-only, API-only, UI-only, and simultaneous writes for 20 rules; run the checker after each runtime generation.
- [ ] Add offline CI mode backed by fixture SQLite and mocked four-source JSON; fail missing ID, duplicate ID, absent runtime evidence, or boolean mismatch.
- [ ] Run `npm run typecheck` and `npm run test` in `clients/web-admin`.
- [ ] Commit checker, tests, and evidence as `audit: enforce four source rule toggle parity`.

### Task 18: Build Sourced Vendor Benchmark Matrix

**Files:**
- Create: `reports/event_alarm_stability_audit_20260930/benchmark_sources.json`
- Create: `reports/event_alarm_stability_audit_20260930/benchmark_matrix.md`

**Interfaces:**
- Produces: 12 dimensions × every requested vendor, with ours/baseline/gap/evidence and explicit unknown cells.

- [ ] Search official Huawei, Hikvision, Dahua, Uniview, Sophgo, Advantech, and Extreme Vision sources.
- [ ] Search official NVIDIA, Intel, BriefCam, Axis, Milestone, Genetec, and Avigilon sources.
- [ ] Search official Ipsotek, Vaidio, IronYun, IntelliVision, and Agent VI sources.
- [ ] Use only official domains, standards bodies, or primary release notes; open every cited page.
- [ ] For each source record vendor, title, URL, publisher, publication date, accessed date, exact section/clause, and `allowed_type`.
- [ ] If a vendor lacks sufficient public evidence, record only “未检索到足以逐项支撑的公开资料” and `unknown`.
- [ ] Use exact dimensions: `popup_latency`, `dedup_suppression_aggregation`, `cooldown`, `sensitivity_confidence_size`, `popup_debounce_merge`, `false_positive_governance`, `event_lifecycle`, `timestamp_management`, `observability`, `roi_consistency`, `ui_db_toggle`, `hardware_degradation`.
- [ ] Separate `vendor claim` from measured baseline; unsupported claims are `not comparable`.
- [ ] For timestamp comparison, cite ONVIF UtcTime and GB 37300-2018 5.9 directly.
- [ ] Commit sources and matrix as `audit: add sourced vendor benchmark matrix`.

### Task 19: Prioritize Defects And Define Regression Gates

**Files:**
- Create: `reports/event_alarm_stability_audit_20260930/defects.csv`
- Create: `reports/event_alarm_stability_audit_20260930/regression_plan.md`
- Modify: `reports/event_alarm_stability_audit_20260930/README.md`

**Interfaces:**
- Consumes: all finding IDs from Tasks 3–17.
- Produces: P0/P1/P2 ledger and executable regression list.

- [ ] Assign P0 to alarm loss, duplicate alarm, ROI ownership drift, toggle drift, unrecoverable stream/TPU/plugin failure, crash, leak, disk loss path, or E2E p95 above 5s.
- [ ] Assign P1 to degraded availability, weak fallback, missing observability, cleanup failure, or recoverable watchdog/scheduling gap.
- [ ] Assign P2 to presentation mismatch, sub-5s latency above target, missing comparison evidence, or maintainability risk.
- [ ] Ensure every row has all four closure fields; otherwise status `audit-gap`.
- [ ] Include exact gates:

```bash
cd /home/brewswang/workshop/SmartGateWay/box-sdk && cmake -S . -B build-test -DCMAKE_BUILD_TYPE=Debug -DBUILD_TESTING=ON && cmake --build build-test -j"$(nproc)" && ctest --test-dir build-test --output-on-failure
cd /home/brewswang/workshop/SmartGateWay/clients/web-admin && npm run typecheck && npm run test && npm run build
cd /home/brewswang/workshop/SmartGateWay && PYTHONPATH=tests/audit python3 -m pytest tests/audit -q
cd /home/brewswang/workshop/SmartGateWay && ./scripts/regression_video_preview.sh
python3 scripts/audit/check_roi_identity.py --api-base http://127.0.0.1:18080 --output reports/event_alarm_stability_audit_20260930/roi_identities.json
python3 scripts/ci/check_rule_toggle_consistency.py --json-output reports/event_alarm_stability_audit_20260930/toggle_consistency.json
```

- [ ] Add BM1688 build, CV186AH build, fault suite, E2E harness, and expected exit codes.
- [ ] Commit ledger and plan as `audit: publish event alarm stability findings`.

### Task 20: Apply P0 Fixes And Final Verification

**Files:**
- Modify: only files named by approved P0 defects.
- Modify: `reports/event_alarm_stability_audit_20260930/README.md`

**Interfaces:**
- Consumes: P0 ledger.
- Produces: fixed code, green gates, and final PASS/FAIL verdict.

- [ ] Select the smallest coherent P0 set; keep total change under 800 lines unless mechanically generated.
- [ ] For each P0, write the failing anchor-specific test before implementation.
- [ ] Implement the root-cause fix and preserve public API compatibility.
- [ ] Run the single changed-project test first, then the full gates from Task 19.
- [ ] Update snapshots only for intentional UI changes and review every snapshot diff.
- [ ] Run live E2E, ROI checker, toggle checker, stream recovery, and resource suite on the isolated device.
- [ ] Mark a P0 closed only after all four defect fields and its regression artifact exist.
- [ ] Compute final status for all 18 subsystems and 12 benchmark dimensions as `verified`, `not_verified`, or `not_comparable`.
- [ ] Search artifacts for `TBD`, `TODO`, unsupported vendor claims, and passes without artifacts.
- [ ] Set final verdict `PASS` only when every P0 is closed, every P1 has an owner, and all 18 subsystems have evidence; otherwise set `FAIL` with blockers.
- [ ] Commit final evidence as `audit: finalize stability release gate`.

## Self-Review

- All 18 requested subsystems map to Tasks 3–17; Task 2 supplies the shared runtime evidence model.
- All five deliverables map to report artifacts and Tasks 18–20.
- ROI “byte-for-byte” is implemented as canonical logical-geometry hashing plus persisted byte checks; rendering may scale pixels, but source coordinates and ownership IDs must be identical.
- Vendors without official evidence remain explicitly unknown; no mechanism is transplanted as fact.
- Every defect requires a real artifact and regression command before `closed`.
