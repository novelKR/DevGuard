#!/usr/bin/env python3
"""Run explicit nonempty functional suites; never infer SLO or unimplemented OS support."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import time
import uuid

from validate import PINNED_RUST, ROOT, source_contract, source_fingerprint

SUITES = {
    "dg1-authority": [
        ("storage", ["-p", "devguard-core", "--test", "authority_storage"]),
        ("activation", ["-p", "devguard-core", "--test", "authority_contract", "storage_activation"]),
        ("paths", ["-p", "devguard-daemon", "--lib", "paths::tests"]),
        ("configuration", ["-p", "devguard-daemon", "--lib", "config::tests"]),
        ("entrypoint", ["-p", "devguard-daemon", "--test", "entrypoint"]),
    ],
    "dg1-auth": [
        ("framing", ["-p", "devguard-client", "--lib"]),
        ("credential-fd", ["-p", "devguard-client", "--test", "credentials"]),
        ("native-peer", ["-p", "devguard-client", "--test", "native_peer"]),
        ("wire-compatibility", ["-p", "devguard-client", "--test", "wire_compatibility"]),
        # Native sampling tests belong to dg1-probes, not the transport suite.
        ("service", ["-p", "devguard-daemon", "--lib", "server::tests", "--",
                     "--skip", "server::tests::native", "--skip", "server::tests::launch",
                     "--skip", "server::tests::lease", "--skip", "server::tests::drain"]),
    ],
    "dg1-probes": [
        ("pressure-controller", ["-p", "devguard-core", "--test", "pressure_contract"]),
        ("sampler", ["-p", "devguard-macos", "--lib", "sampler::tests"]),
        ("clock-identity", ["-p", "devguard-macos", "--test", "identity"],
         ["boot-clock", "host-capacity", "process-identity"]),
        ("host-pressure", ["-p", "devguard-macos", "--test", "pressure"],
         ["native-pressure-readings", "admission-closes-on-stale-samples", "admission-closes-on-probe-failure"]),
        ("native-registration", ["-p", "devguard-daemon", "--test", "native_authority"]),
        ("service-sampling", ["-p", "devguard-daemon", "--lib", "server::tests::native"],
         ["service-probe-failure", "service-delayed-probe"]),
    ],
    "dg1-scopes": [
        ("scope-logic", ["-p", "devguard-macos", "--lib", "scope::tests"]),
        ("native-scopes", ["-p", "devguard-macos", "--test", "scopes"],
         ["scope-lifecycle", "capability-matrix", "scope-escape", "scope-refusals"]),
    ],
    "dg1-launch": [
        ("helper-claims", ["-p", "devguard-core", "--test", "authority_contract", "helper_claim"]),
        ("launch-cancel", ["-p", "devguard-core", "--test", "authority_contract", "launch_cancel"]),
        ("helper-identity", ["-p", "devguard-macos", "--lib", "scope::tests::launch_helper"]),
        ("transcript", ["-p", "devguard-client", "--lib", "launch::tests"]),
        ("wire-launch", ["-p", "devguard-client", "--test", "wire_compatibility", "launch"]),
        ("service-sessions", ["-p", "devguard-daemon", "--lib", "server::tests::launch"]),
        ("native-launch", ["-p", "devguard-launch", "--test", "launch"],
         ["launch-lifecycle", "payload-inventory", "duplicate-helpers", "helper-refusals",
          "cancelled-before-claim", "exec-failure", "lost-authorization-replay",
          "claimed-then-refused", "pseudo-terminal", "deadline-missed", "concurrent-launches"]),
    ],
    "dg1-cli": [
        ("cli-arguments", ["-p", "devguard-cli", "--lib", "args::tests"]),
        ("cli-preflight", ["-p", "devguard-cli", "--lib", "preflight::tests"]),
        ("cli-reply-races", ["-p", "devguard-cli", "--lib", "exec::tests"]),
        ("cli-entrypoint", ["-p", "devguard-cli", "--test", "entrypoint"]),
        ("native-exec", ["-p", "devguard-cli", "--test", "exec"],
         ["exec-lifecycle", "signal-death", "signal-forwarding", "ignored-signals",
          "sigstop-not-mirrored", "observe-before-reap", "refused-budget",
          "wait-beyond-capacity", "wait-admitted", "wait-deadline", "wait-cancelled",
          "unavailable-authority", "capability-mismatch", "doctor", "project-resolution",
          "instance-pool", "sequential-owners", "unstartable-programs"]),
        ("native-terminal", ["-p", "devguard-cli", "--test", "terminal"],
         ["terminal-interrupt", "terminal-stop", "terminal-output-only"]),
    ],
    "dg1-cargo": [
        ("cargo-plan", ["-p", "devguard-cargo", "--lib"]),
        ("cargo-adapters", ["-p", "devguard-cli", "--lib", "adapter::tests"]),
        ("native-cargo", ["-p", "devguard-cli", "--test", "cargo"],
         ["direct-build", "explicit-jobs", "cargo-refusals", "cargo-test", "pipeline-shared",
          "nested-cargo", "inherited-jobserver", "inherited-pipe-jobserver", "stale-jobserver",
          "concurrent-consumers", "pipeline-cancelled"]),
    ],
    "dg1-bootstrap": [
        ("install-logic", ["-p", "devguard-daemon", "--lib", "install::tests"]),
        ("native-install", ["-p", "devguard-daemon", "--test", "install"],
         ["install-verified", "install-reuse", "install-refusals", "install-preconditions",
          "install-unverified", "install-concurrent", "launchd-lifecycle", "install-unrecorded"]),
    ],
    "dg1-reconcile": [
        ("reconcile-contract", ["-p", "devguard-core", "--test", "authority_contract", "reconcile_"]),
        ("launcher-evidence", ["-p", "devguard-macos", "--lib", "backend::tests"]),
        ("wire-reconcile", ["-p", "devguard-client", "--test", "wire_compatibility", "reconcile"]),
        ("owner-liveness", ["-p", "devguard-daemon", "--lib", "server::tests::a_process_of_an_earlier_boot"]),
        ("service-reconciler", ["-p", "devguard-daemon", "--lib",
                                "server::tests::launch::launch_reconciler_stops"]),
        ("native-reconcile", ["-p", "devguard-launch", "--test", "reconcile"],
         ["prepared-cancel-expiry", "no-helper-created", "claimed-abandonment",
          "observe-before-reap", "reaped-before-observation", "known-escape", "terminate-scope",
          "cancel-after-authorization", "dead-owner", "retired-instance", "unresponsive-helper",
          "daemon-crash-restart", "journal-failure"]),
    ],
    "dg1-self-use": [
        ("lease-contract", ["-p", "devguard-core", "--test", "lease_contract"]),
        ("wire-lease", ["-p", "devguard-client", "--test", "wire_compatibility", "lease_"]),
        ("service-leases", ["-p", "devguard-daemon", "--lib", "server::tests::lease"]),
        ("candidate-service", ["-p", "devguard-daemon", "--lib", "candidate::tests"]),
        ("candidate-entrypoint", ["-p", "devguard-daemon", "--test", "entrypoint", "a_candidate_"]),
        ("lease-arguments", ["-p", "devguard-cli", "--lib", "args::tests::lease_"]),
        ("candidate-plan", ["-p", "devguard-cli", "--lib", "candidate::tests"]),
        ("native-self-use", ["-p", "devguard-cli", "--test", "candidate"],
         ["test-candidate", "lease-children", "test-candidate-crash", "test-candidate-stopped"]),
    ],
    "dg1-upgrade": [
        ("quiescence-contract", ["-p", "devguard-core", "--test", "quiescence_contract"]),
        ("wire-drain", ["-p", "devguard-client", "--test", "wire_compatibility", "drain_"]),
        ("service-drain", ["-p", "devguard-daemon", "--lib", "server::tests::drain"]),
        ("upgrade-logic", ["-p", "devguard-daemon", "--lib", "upgrade::tests"]),
        ("upgrade-arguments", ["-p", "devguard-cli", "--lib", "args::tests::upgrade_"]),
        ("native-upgrade", ["-p", "devguard-daemon", "--test", "upgrade"],
         ["upgrade-normal", "drain-timeout", "upgrade-unverified", "downgrade-refused",
          "upgrade-stopped", "repair-recovery", "repair-corrupt-journal", "repair-previous",
          "upgrade-after-recovery", "stop-failed", "repair-closure", "upgrade-completed",
          "upgrade-cancelled", "upgrade-died-verifying", "repair-completed"]),
    ],
    "dg1-macos": [
        ("workloads", ["-p", "devguard-qualify", "--lib"]),
        ("protocol-analysis", ["unittest", "test_measure.py"]),
        ("native-control", ["-p", "devguard-qualify", "--test", "control"],
         ["control-probe", "control-stopped", "control-refused", "control-stopped-mid-run",
          "control-helper-exited"]),
        ("native-fixture", ["unittest", "test_fixture.py"], ["fixture-check"]),
    ],
}
# Binaries a suite's tests start but whose package the suite does not test.
PREBUILD = {
    "dg1-cli": [["-p", "devguard-launch", "--bin", "devguard-launch"]],
    "dg1-cargo": [["-p", "devguard-launch", "--bin", "devguard-launch"]],
    "dg1-self-use": [["-p", "devguard-launch", "--bin", "devguard-launch"]],
    "dg1-macos": [["-p", "devguard-launch", "--bin", "devguard-launch"]],
}
STAGE_TIMEOUT_SECONDS = 1800
SCOPES = {
    "dg1-authority": "canonical configuration, ownership and storage",
    "dg1-auth": "native UDS peer/credential transport and closed readiness",
    "dg1-probes": "native macOS boot clock, process identity and host pressure evidence with registration closed",
    "dg1-scopes": "native macOS cooperative CPU policy readback, observed process-group scopes and identity-checked termination without a launch helper",
    "dg1-launch": "native macOS launch helper through an isolated authority: one claimed helper per grant, and scope binding and authorization before READY and exec",
    "dg1-cli": "the devguard command-line owner against isolated authorities with the real launch helper: preserved argv, directory, environment and exit status, signal forwarding and terminal job control, observation before reap, refusals, bounded waits, projects, the instance pool and doctor diagnostics",
    "dg1-cargo": "the Cargo adapters through the devguard owner against isolated authorities with real Cargo builds: jobs fitted to the reservation, clamping and refusals, one jobserver shared by a pipeline and by nested Cargo, inherited and stale jobservers, concurrent consumers and cancellation; Cargo jobs are compilation parallelism, not a cap on test threads or measured memory",
    "dg1-bootstrap": "installation of a release as the current user's LaunchAgent against isolated authorities: package validation, immutable release and recovery copies, verification that launchd runs the release's own binary before it is selected, refusals, concurrent installers, and under launchd itself a crash restart, a SIGTERM stop and a fail-closed start that is not restarted; functional artifacts only, not SLO qualification",
    "dg1-reconcile": "native macOS reconciliation through isolated authorities: prepared cancellation and expiry, owner reports that no helper exists, releases only on scope termination, sticky escape and tracking loss, scope termination signals, dead owners, and a daemon crash with restart",
    "dg1-upgrade": "replacement and repair of the installed service against isolated authorities with a fake service manager: compatibility checks that refuse an incompatible downgrade, admission closed by an administrator and kept across a restart, drains that finish or time out with every charge kept, quiescent backups, a new release started closed and verified before admission reopens, a failed start or stop, or a release that dies while it is verified, giving way to the previous release on the same journal, a drain cancelled by a signal, an interrupted upgrade completed by running it again, a release that cannot drain replaced only when stopped with nothing charged, and repair that never starts a second authority, completes an interrupted repair, returns to the release an upgrade replaced, uses the recovery copy of a damaged release and keeps a journal that cannot be opened closed; functional fixtures only, not the real upgrade of the installed service",
    "dg1-macos": "the SLO protocol's harness against isolated authorities: bounded workloads, the standalone control probe that owns its targets and measures status and termination acknowledgement, the protocol's percentile, validity and verdict rules, and the foreground fixture observed through headless Chrome, which is never a valid observation; the SLO protocol itself is scripts/measure.py macos on the target host",
    "dg1-self-use": "parent leases and candidate authorities against isolated authorities: a lease charged once against the host, children admitted only against its remainder with its token, fencing when it ends, its owner goes or its deadline passes, release once every child is settled, and a candidate authority run as a lease child through the real launch helper that admits within its leased capacity, launches nothing and closes with its lease; functional fixtures only, not real self-use under the installed parent",
}
# Suites that start real workloads through the launch helper (in fixtures).
LAUNCHING = {"dg1-launch", "dg1-reconcile", "dg1-cli", "dg1-cargo", "dg1-self-use", "dg1-macos"}
# Native suites observe the actual host; elsewhere they are not run, never passed.
NATIVE = {"dg1-probes", "dg1-scopes", "dg1-launch", "dg1-reconcile", "dg1-cli", "dg1-cargo", "dg1-bootstrap",
          "dg1-self-use", "dg1-upgrade", "dg1-macos"}


def host_facts():
    uname = os.uname()
    facts = {"platform": platform.platform(), "machine": platform.machine(),
             "system": uname.sysname, "release": uname.release, "version": uname.version}
    if platform.mac_ver()[0]:
        facts["macos"] = platform.mac_ver()[0]
    return facts


def not_run_cases(output, files):
    """Receipts may record a case the environment could not produce as not run."""
    found = []

    def walk(value, where):
        if isinstance(value, dict):
            if value.get("status") == "not_run":
                found.append({"file": where[0], "path": "/".join(where[1:]), "reason": value.get("reason")})
            for key, item in value.items():
                walk(item, where + [key])
        elif isinstance(value, list):
            for index, item in enumerate(value):
                walk(item, where + [str(index)])

    for path in files:
        walk(json.loads(path.read_text()), [str(path.relative_to(output))])
    return found


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("suite",choices=sorted(SUITES))
    parser.add_argument("--offline",action="store_true")
    parser.add_argument("--output",type=Path)
    # Only for environments known to be unable to produce a case (for example
    # hosted runners that clamp every process): the report stays incomplete.
    parser.add_argument("--allow-incomplete",action="store_true")
    args=parser.parse_args()
    run_id=datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")+"-"+uuid.uuid4().hex[:8]
    output=(args.output or ROOT/"target/qualification"/(args.suite+"-"+run_id)).resolve()
    if not output.is_relative_to(ROOT) or subprocess.run(["git","check-ignore","-q",str(output/"report.json")],cwd=ROOT).returncode:
        parser.error("output must be a new ignored path inside this checkout")
    output.mkdir(parents=True,exist_ok=False)
    report={"schema":"devguard-functional-qualification/v1","suite":args.suite,"run_id":run_id,
            "status":"failed","execution_mode":"bootstrap-functional-tests","self_governed":False,
            "scope":SCOPES[args.suite] + ("; no kernel resource-control enforcement or SLO qualification" if args.suite in LAUNCHING
                                          else "; no workload launch, resource-control enforcement or SLO qualification"),
            "runtime_qualification":{"macos_launch":"functional_fixture" if args.suite in LAUNCHING else "not_run",
                                     "linux_cgroups":"not_run","foreground_slo":"not_run",
                                     "candidate_self_use":"functional_fixture" if args.suite=="dg1-self-use" else "not_run"},"stages":[]}
    environment=os.environ.copy()
    environment.update(CARGO_BUILD_JOBS="1",RUST_TEST_THREADS="1")
    try:
        before=source_fingerprint();report["source"]=before
        report["rustc"]=subprocess.check_output(["rustc","--version"],text=True,env=environment).strip()
        if report["rustc"].split()[1]!=PINNED_RUST: raise RuntimeError("functional qualification requires Rust "+PINNED_RUST)
        source_contract()
        report["host"]=host_facts()
        if args.suite in NATIVE and platform.system()!="Darwin":
            report["status"]="not_run"
            report["reason"]="native macOS suite; this platform cannot supply the evidence"
            return 2
        skipped=[]
        for index,selectors in enumerate(PREBUILD.get(args.suite,[])):
            command=["cargo","build","--locked",*(["--offline"] if args.offline else []),*selectors]
            entry={"name":f"prebuild-{index}","command":command,"status":"failed","log":f"prebuild-{index}.log"}
            report.setdefault("prebuild",[]).append(entry)
            with (output/entry["log"]).open("w") as stream:
                result=subprocess.run(command,cwd=ROOT,env=environment,stdout=stream,stderr=subprocess.STDOUT,timeout=STAGE_TIMEOUT_SECONDS)
            entry["log_sha256"]=hashlib.sha256((output/entry["log"]).read_bytes()).hexdigest()
            if result.returncode: raise RuntimeError("prebuild failed: "+" ".join(selectors))
            entry["status"]="passed"
        for name,selectors,*expected in SUITES[args.suite]:
            expected=expected[0] if expected else None
            if selectors[0]=="unittest":
                command=[sys.executable,"-B","-m","unittest","discover","-s","scripts","-p",selectors[1]]
            else:
                command=["cargo","test","--locked",*(["--offline"] if args.offline else []),*selectors]
            stage={"name":name,"command":command,"status":"failed","log":name+".log"}
            report["stages"].append(stage);started=time.monotonic()
            stage_environment=dict(environment)
            raw=output/"raw"/name
            if expected:
                raw.mkdir(parents=True)
                stage_environment["DEVGUARD_EVIDENCE_DIR"]=str(raw)
            with (output/stage["log"]).open("w") as stream:
                result=subprocess.run(command,cwd=ROOT,env=stage_environment,stdout=stream,stderr=subprocess.STDOUT,timeout=STAGE_TIMEOUT_SECONDS)
            stage["seconds"]=round(time.monotonic()-started,3)
            log=(output/stage["log"])
            stage["log_sha256"]=hashlib.sha256(log.read_bytes()).hexdigest()
            text=log.read_text(errors="replace")
            stage["tests_passed"]=sum(map(int,re.findall(r"test result: ok\. (\d+) passed",text)))
            if selectors[0]=="unittest" and re.search(r"^OK( \(.*\))?$",text,re.M):
                # Skipped tests ran nothing, so they are not counted as passed.
                skipped_tests=sum(map(int,re.findall(r"skipped=(\d+)",text)))
                stage["tests_passed"]=sum(map(int,re.findall(r"^Ran (\d+) tests? in",text,re.M)))-skipped_tests
            if result.returncode or not stage["tests_passed"]: raise RuntimeError("failed or empty suite: "+name)
            if expected:
                files=sorted(path for path in raw.iterdir() if path.is_file())
                missing=sorted(set(expected)-{path.stem for path in files})
                if missing: raise RuntimeError("native stage "+name+" is missing raw evidence: "+", ".join(missing))
                stage["raw_evidence"]={str(path.relative_to(output)):hashlib.sha256(path.read_bytes()).hexdigest() for path in files}
                stage["not_run_cases"]=not_run_cases(output,files)
                skipped.extend(stage["not_run_cases"])
            stage["status"]="incomplete" if stage.get("not_run_cases") else "passed"
            print(name+": "+stage["status"]+" ("+str(stage["tests_passed"])+")",flush=True)
        if source_fingerprint()!=before: raise RuntimeError("source changed during qualification")
        # A case the environment could not produce is not a pass.
        report["status"]="incomplete" if skipped else "passed"
    except (OSError,ValueError,RuntimeError,subprocess.CalledProcessError,subprocess.TimeoutExpired) as error:
        report["error"]=str(error)
    finally:
        (output/"report.json").write_text(json.dumps(report,indent=2)+"\n")
        print(json.dumps({"status":report["status"],"report":str(output/"report.json")}))
    if report["status"]=="incomplete" and args.allow_incomplete:
        print("incomplete accepted by --allow-incomplete; the report still records it",flush=True)
        return 0
    return {"passed":0,"incomplete":2,"not_run":2}.get(report["status"],1)


if __name__=="__main__":
    sys.exit(main())
