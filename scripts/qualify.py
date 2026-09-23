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
        ("service", ["-p", "devguard-daemon", "--lib", "server::tests", "--", "--skip", "server::tests::native"]),
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
}
STAGE_TIMEOUT_SECONDS = 1800
SCOPES = {
    "dg1-authority": "canonical configuration, ownership and storage",
    "dg1-auth": "native UDS peer/credential transport and closed readiness",
    "dg1-probes": "native macOS boot clock, process identity and host pressure evidence with registration closed",
    "dg1-scopes": "native macOS cooperative CPU policy readback, observed process-group scopes and identity-checked termination without a launch helper",
}
# Native suites observe the actual host; elsewhere they are not run, never passed.
NATIVE = {"dg1-probes", "dg1-scopes"}


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
            "scope":SCOPES[args.suite] + "; no workload launch, resource-control enforcement or SLO qualification",
            "runtime_qualification":{"macos_launch":"not_run","linux_cgroups":"not_run","foreground_slo":"not_run","candidate_self_use":"not_run"},"stages":[]}
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
        for name,selectors,*expected in SUITES[args.suite]:
            expected=expected[0] if expected else None
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
            stage["tests_passed"]=sum(map(int,re.findall(r"test result: ok\. (\d+) passed",log.read_text(errors="replace"))))
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
