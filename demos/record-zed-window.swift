// Actual window-only ScreenCaptureKit capture. No desktop crop, audio or input synthesis.
import Foundation
import ScreenCaptureKit
import AVFoundation
import Darwin
import CoreGraphics

private struct CaptureError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}

private func permittedWindow(bundle: String?, title: String?) -> Bool {
    bundle == "dev.zed.Zed" && (title == "demo — trim-long.md" || title == "demo — writing.md" || title == "demo — writing.clean.md")
}

private final class Watchdog {
    private let lock = NSLock()
    private var deadline = Date().addingTimeInterval(15)
    private let timer = DispatchSource.makeTimerSource(queue: .global())
    init() {
        timer.schedule(deadline: .now(), repeating: .milliseconds(100))
        timer.setEventHandler { [weak self] in
            guard let self = self else { return }
            self.lock.lock(); let expired = Date() > self.deadline; self.lock.unlock()
            if expired {
                fputs("Window recording hard timeout; partial output is NOT accepted.\n", stderr)
                exit(2)
            }
        }
        timer.resume()
    }
    func arm(_ seconds: Double) {
        lock.lock(); deadline = Date().addingTimeInterval(seconds); lock.unlock()
    }
    func cancel() { timer.cancel() }
}

private func publicWindowStillSelected(_ id: CGWindowID) -> Bool {
    guard let items = CGWindowListCopyWindowInfo([.optionIncludingWindow], id) as? [[String: Any]],
          let info = items.first,
          let title = info[kCGWindowName as String] as? String else { return false }
    return permittedWindow(bundle: "dev.zed.Zed", title: title)
}

private final class RecordingDelegate: NSObject, SCRecordingOutputDelegate {
    private let lock = NSLock()
    private var started = false
    private var finished = false
    private var failure: String?
    func recordingOutputDidStartRecording(_ recordingOutput: SCRecordingOutput) {
        lock.lock(); started = true; lock.unlock()
        print("WINDOW_RECORDING_STARTED"); fflush(stdout)
    }
    func recordingOutputDidFinishRecording(_ recordingOutput: SCRecordingOutput) {
        lock.lock(); finished = true; lock.unlock()
    }
    func recordingOutput(_ recordingOutput: SCRecordingOutput, didFailWithError error: Error) {
        lock.lock(); failure = String(describing: error); lock.unlock()
    }
    func state() -> (Bool, Bool, String?) {
        lock.lock(); defer { lock.unlock() }
        return (started, finished, failure)
    }
}

@main
private struct WindowRecorder {
    static func main() async {
        let watchdog = Watchdog()
        var activeStream: SCStream?
        do {
            let args = CommandLine.arguments
            if args.count == 2 && args[1] == "--self-test" {
                precondition(permittedWindow(bundle: "dev.zed.Zed", title: "demo — writing.md"))
                precondition(permittedWindow(bundle: "dev.zed.Zed", title: "demo — trim-long.md"))
                precondition(permittedWindow(bundle: "dev.zed.Zed", title: "demo — writing.clean.md"))
                precondition(!permittedWindow(bundle: "com.apple.Safari", title: "demo — writing.md"))
                precondition(!permittedWindow(bundle: "dev.zed.Zed", title: "personal — writing.md"))
                precondition(!permittedWindow(bundle: "dev.zed.Zed", title: nil))
                print("Window allowlist tests: 6 passed")
                watchdog.cancel()
                return
            }
            guard args.count == 4, let duration = Double(args[2]), duration >= 3, duration <= 1800 else {
                throw CaptureError("Usage: record-zed-window OUTPUT.mp4 DURATION_SECONDS STOP_FILE (3...1800 seconds)")
            }
            let output = URL(fileURLWithPath: args[1]).standardizedFileURL
            let stop = URL(fileURLWithPath: args[3]).standardizedFileURL
            let fm = FileManager.default
            guard output.pathExtension == "mp4", !fm.fileExists(atPath: output.path),
                  !fm.fileExists(atPath: stop.path),
                  output.resolvingSymlinksInPath() == output,
                  stop.resolvingSymlinksInPath() == stop else {
                throw CaptureError("Fresh regular output/stop paths required; existing paths and symlinks refused")
            }
            // Only the three exact public demo titles are eligible; no unrelated window metadata is printed.
            let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
            let windows = content.windows.filter {
                permittedWindow(bundle: $0.owningApplication?.bundleIdentifier, title: $0.title)
            }
            guard windows.count == 1, let window = windows.first else {
                throw CaptureError("Exactly one on-screen public Zed demo window required; found \(windows.count)")
            }
            guard publicWindowStillSelected(window.windowID) else {
                throw CaptureError("Public window title is unavailable or changed before capture")
            }
            let filter = SCContentFilter(desktopIndependentWindow: window)
            let config = SCStreamConfiguration()
            config.width = 1140
            config.height = 720
            config.minimumFrameInterval = CMTime(value: 1, timescale: 15)
            config.showsCursor = true
            config.capturesAudio = false
            config.captureMicrophone = false
            config.includeChildWindows = false
            config.queueDepth = 5
            let stream = SCStream(filter: filter, configuration: config, delegate: nil)
            let recordingConfig = SCRecordingOutputConfiguration()
            recordingConfig.outputURL = output
            recordingConfig.videoCodecType = .h264
            recordingConfig.outputFileType = .mp4
            let delegate = RecordingDelegate()
            let recording = SCRecordingOutput(configuration: recordingConfig, delegate: delegate)
            try stream.addRecordingOutput(recording)
            activeStream = stream
            watchdog.arm(15)
            try await stream.startCapture()
            guard publicWindowStillSelected(window.windowID) else {
                throw CaptureError("Public Zed window changed during capture startup")
            }
            watchdog.arm(duration + 15)
            let begin = Date()
            while Date().timeIntervalSince(begin) < duration && !fm.fileExists(atPath: stop.path) {
                guard publicWindowStillSelected(window.windowID) else {
                    throw CaptureError("Public Zed window changed or disappeared; partial capture refused")
                }
                let state = delegate.state()
                if let failure = state.2 { throw CaptureError(failure) }
                if !state.0 && Date().timeIntervalSince(begin) > 10 {
                    throw CaptureError("Recording start acknowledgement absent after 10 seconds")
                }
                try await Task.sleep(nanoseconds: 100_000_000)
            }
            watchdog.arm(15)
            try await stream.stopCapture()
            activeStream = nil
            let deadline = Date().addingTimeInterval(10)
            while !delegate.state().1 && Date() < deadline {
                if let failure = delegate.state().2 { throw CaptureError(failure) }
                try await Task.sleep(nanoseconds: 100_000_000)
            }
            guard publicWindowStillSelected(window.windowID) else {
                throw CaptureError("Public Zed window changed during recording finalization")
            }
            let state = delegate.state()
            guard state.0 && state.1 && state.2 == nil else {
                throw CaptureError("Recording completion acknowledgement missing or failed")
            }
            watchdog.cancel()
            print("WINDOW_RECORDING_FINISHED duration=\(CMTimeGetSeconds(recording.recordedDuration)) bytes=\(recording.recordedFileSize)")
        } catch {
            watchdog.arm(10)
            if let stream = activeStream { try? await stream.stopCapture() }
            watchdog.cancel()
            fputs("Window recording failed: \(error)\n", stderr)
            exit(1)
        }
    }
}
