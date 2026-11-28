import json
import os
import shutil
import signal
import subprocess
import sys
import time
import urllib.request

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
RESULTS_DIR = os.path.join(BASE_DIR, "results")
RECORDINGS_DIR = os.path.join(BASE_DIR, "recordings")
os.makedirs(RESULTS_DIR, exist_ok=True)
os.makedirs(RECORDINGS_DIR, exist_ok=True)

SERVER_URL = "http://127.0.0.1:8877"

def wait_for_server():
    for _ in range(30):
        try:
            with urllib.request.urlopen(f"{SERVER_URL}/api/ram?browser=test", timeout=1) as r:
                if r.status == 200:
                    return True
        except Exception:
            time.sleep(0.2)
    raise RuntimeError("Benchmark server did not start on port 8877")

def reset_ram(browser):
    try:
        urllib.request.urlopen(f"{SERVER_URL}/api/reset?browser={browser}", timeout=2).read()
    except Exception:
        pass

def cleanup_profile(browser):
    p = f"/tmp/bench-{browser}"
    shutil.rmtree(p, ignore_errors=True)
    os.makedirs(p, exist_ok=True)
    if browser == "firefox":
        with open(os.path.join(p, "user.js"), "w") as f:
            f.write('user_pref("browser.startup.homepage_override.mstone", "ignore");\n')
            f.write('user_pref("datareporting.policy.firstRunURL", "");\n')
            f.write('user_pref("browser.shell.checkDefaultBrowser", false);\n')
            f.write('user_pref("browser.tabs.warnOnClose", false);\n')
            f.write('user_pref("toolkit.telemetry.reportingpolicy.firstRun", false);\n')
            f.write('user_pref("browser.aboutwelcome.enabled", false);\n')
            f.write('user_pref("layout.frame_rate", 0);\n')
    return p

def kill_browser(browser):
    if browser == "anarc":
        subprocess.run(["pkill", "-TERM", "-f", "/home/rover/.local/share/anarc/bin/anarc"], stderr=subprocess.DEVNULL)
        time.sleep(1.0)
        subprocess.run(["pkill", "-KILL", "-f", "/home/rover/.local/share/anarc/bin/anarc"], stderr=subprocess.DEVNULL)
    else:
        pattern = f"/tmp/bench-{browser}"
        subprocess.run(["pkill", "-TERM", "-f", pattern], stderr=subprocess.DEVNULL)
        time.sleep(1.0)
        subprocess.run(["pkill", "-KILL", "-f", pattern], stderr=subprocess.DEVNULL)

def run_single(browser):
    print(f"\n=== Starting benchmark for: {browser} ===", flush=True)
    result_path = os.path.join(RESULTS_DIR, f"{browser}.json")
    if os.path.exists(result_path):
        os.remove(result_path)
    rec_path = os.path.join(RECORDINGS_DIR, f"{browser}.mp4")
    if os.path.exists(rec_path):
        os.remove(rec_path)

    kill_browser(browser)
    profile_dir = cleanup_profile(browser)
    reset_ram(browser)

    # Start 1080p @ 60 FPS screen recording
    gsr_cmd = [
        "gpu-screen-recorder",
        "-w", "eDP-1",
        "-f", "60",
        "-c", "mp4",
        "-q", "very_high",
        "-o", rec_path,
    ]
    gsr_proc = subprocess.Popen(gsr_cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(0.6)

    launch_ts = int(time.time() * 1000)
    main_url = f"{SERVER_URL}/index.html?browser={browser}&launch_ts={launch_ts}"
    env = os.environ.copy()

    if browser == "chromium":
        cmd = [
            "/usr/bin/chromium",
            "--ozone-platform=wayland",
            "--disable-gpu-vsync",
            "--disable-frame-rate-limit",
            f"--user-data-dir={profile_dir}",
            "--no-first-run",
            "--no-default-browser-check",
            "--new-window",
            "--start-maximized",
            main_url,
        ]
        b_proc = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    elif browser == "brave":
        cmd = [
            "/usr/bin/brave",
            "--ozone-platform=wayland",
            "--disable-gpu-vsync",
            "--disable-frame-rate-limit",
            f"--user-data-dir={profile_dir}",
            "--no-first-run",
            "--no-default-browser-check",
            "--new-window",
            "--start-maximized",
            main_url,
        ]
        b_proc = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    elif browser == "firefox":
        env["MOZ_ENABLE_WAYLAND"] = "1"
        cmd = [
            "/usr/bin/firefox",
            "-profile", profile_dir,
            "-no-remote",
            "-new-instance",
            main_url,
        ]
        b_proc = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    elif browser == "anarc":
        env["ANARC_UNCAPPED_FPS"] = "1"
        env["ANARC_OSR_HEADROOM_PERCENT"] = "0"
        env["ANARC_CEF_INIT_DELAY_MS"] = "0"
        cmd = [
            "/home/rover/.local/bin/anarc",
            "--no-telemetry",
            main_url,
        ]
        b_proc = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        # Find the exact ctl socket belonging to b_proc (never auto-launch a second instance)
        sock_dir = f"/run/user/{os.getuid()}/anarc"
        sock_path = None
        for _ in range(50):
            if os.path.isdir(sock_dir):
                for fn in os.listdir(sock_dir):
                    if fn.endswith(".sock.owner"):
                        owner_file = os.path.join(sock_dir, fn)
                        try:
                            with open(owner_file) as f:
                                if f.read().strip() == str(b_proc.pid):
                                    candidate = os.path.join(sock_dir, fn[:-6])
                                    if os.path.exists(candidate):
                                        sock_path = candidate
                                        break
                        except Exception:
                            pass
            if sock_path:
                break
            time.sleep(0.2)
        if sock_path:
            time.sleep(0.5)
            tabs_out = subprocess.run(["anarc-ctl", "--socket", sock_path, "tabs"], capture_output=True, text=True).stdout
            first_tab_id = None
            for line in tabs_out.splitlines():
                if "index.html" in line or "127.0.0.1:8877" in line:
                    first_tab_id = line.split()[0].strip("[]*")
                    break
            if first_tab_id:
                subprocess.run(["anarc-ctl", "--socket", sock_path, "tab", "select", first_tab_id], capture_output=True)
    else:
        raise ValueError(f"Unknown browser: {browser}")

    # Wait for 40s benchmark + startup to write results/<browser>.json
    deadline = time.time() + 62
    got_result = False
    while time.time() < deadline:
        if os.path.exists(result_path):
            got_result = True
            break
        time.sleep(0.5)

    # Keep summary modal on screen for 2s in recording
    time.sleep(2.0)

    # Stop recording cleanly via SIGINT
    gsr_proc.send_signal(signal.SIGINT)
    try:
        gsr_proc.wait(timeout=8)
    except subprocess.TimeoutExpired:
        gsr_proc.kill()

    kill_browser(browser)
    try:
        b_proc.wait(timeout=3)
    except Exception:
        pass

    if not got_result:
        print(f"ERROR: {browser} did not submit benchmark report within timeout!", flush=True)
        return False

    with open(result_path) as f:
        data = json.load(f)
    print(
        f"DONE {browser}: Startup={data.get('coldStartupMs')}ms, "
        f"MeanFPS={data.get('meanFps')}, 1%Low={data.get('p1LowFps')}, "
        f"Drop={data.get('droppedPct')}%, PeakRAM={data.get('peakRssMb')}MB, "
        f"Score={data.get('compositeScore')}",
        flush=True,
    )
    return True

def main():
    targets = sys.argv[1:] if len(sys.argv) > 1 else ["chromium", "brave", "firefox", "anarc"]
    server_proc = subprocess.Popen(
        ["node", os.path.join(BASE_DIR, "server.mjs")],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    try:
        wait_for_server()
        for b in targets:
            run_single(b)
            time.sleep(2.0)
    finally:
        server_proc.terminate()
        try:
            server_proc.wait(timeout=3)
        except Exception:
            server_proc.kill()

if __name__ == "__main__":
    main()
