#!/usr/bin/env python3
"""Run explicit nonempty functional suites; never infer SLO or unimplemented OS support."""
import argparse
import datetime
import json
import os
from pathlib import Path
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
        ("service", ["-p", "devguard-daemon", "--lib", "server::tests"]),
    ],
}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("suite",choices=sorted(SUITES))
    parser.add_argument("--offline",action="store_true")
    parser.add_argument("--output",type=Path)
    args=parser.parse_args()
    run_id=datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")+"-"+uuid.uuid4().hex[:8]
    output=(args.output or ROOT/"target/qualification"/(args.suite+"-"+run_id)).resolve()
    if not output.is_relative_to(ROOT) or subprocess.run(["git","check-ignore","-q",str(output/"report.json")],cwd=ROOT).returncode:
        parser.error("output must be a new ignored path inside this checkout")
    output.mkdir(parents=True,exist_ok=False)
    report={"schema":"devguard-functional-qualification/v1","suite":args.suite,"run_id":run_id,
            "status":"failed","execution_mode":"bootstrap-functional-tests","self_governed":False,
            "scope":("canonical configuration, ownership and storage" if args.suite == "dg1-authority" else "native UDS peer/credential transport and closed readiness") + "; no native resource-control or SLO qualification",
            "runtime_qualification":{"macos_launch":"not_run","linux_cgroups":"not_run","foreground_slo":"not_run","candidate_self_use":"not_run"},"stages":[]}
    environment=os.environ.copy()
    environment.update(CARGO_BUILD_JOBS="1",RUST_TEST_THREADS="1")
    try:
        before=source_fingerprint();report["source"]=before
        report["rustc"]=subprocess.check_output(["rustc","--version"],text=True,env=environment).strip()
        if report["rustc"].split()[1]!=PINNED_RUST: raise RuntimeError("functional qualification requires Rust "+PINNED_RUST)
        source_contract()
        for name,selectors in SUITES[args.suite]:
            command=["cargo","test","--locked",*(["--offline"] if args.offline else []),*selectors]
            stage={"name":name,"command":command,"status":"failed","log":name+".log"}
            report["stages"].append(stage);started=time.monotonic()
            with (output/stage["log"]).open("w") as stream:
                result=subprocess.run(command,cwd=ROOT,env=environment,stdout=stream,stderr=subprocess.STDOUT)
            stage["seconds"]=round(time.monotonic()-started,3)
            stage["tests_passed"]=sum(map(int,re.findall(r"test result: ok\. (\d+) passed",(output/stage["log"]).read_text(errors="replace"))))
            if result.returncode or not stage["tests_passed"]: raise RuntimeError("failed or empty suite: "+name)
            stage["status"]="passed"
            print(name+": passed ("+str(stage["tests_passed"])+")",flush=True)
        if source_fingerprint()!=before: raise RuntimeError("source changed during qualification")
        report["status"]="passed"
    except (OSError,ValueError,RuntimeError,subprocess.CalledProcessError) as error:
        report["error"]=str(error)
    finally:
        (output/"report.json").write_text(json.dumps(report,indent=2)+"\n")
        print(json.dumps({"status":report["status"],"report":str(output/"report.json")}))
    return 0 if report["status"]=="passed" else 1


if __name__=="__main__":
    sys.exit(main())
