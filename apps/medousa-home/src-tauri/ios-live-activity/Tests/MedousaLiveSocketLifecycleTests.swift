import Foundation

@main
enum MedousaLiveSocketLifecycleTests {
    static func main() {
        var state = MedousaLiveSocketLifecycle()
        precondition(!state.acceptsMicrophone)
        precondition(state.begin())
        precondition(!state.begin())
        precondition(!state.receive(["type": "session.output_audio.delta"]))
        precondition(!state.receive(["type": "session.started", "session": ["id": ""]]))
        precondition(!state.acceptsMicrophone)
        precondition(state.receive(["type": "session.started", "session": ["id": "live-test"]]))
        precondition(state.sessionId == "live-test" && state.acceptsMicrophone)
        precondition(!state.receive(["type": "session.started", "session": ["id": "late"]]))
        state.setMuted(true)
        precondition(!state.acceptsMicrophone)
        state.setMuted(false)
        precondition(state.acceptsMicrophone)
        precondition(state.close())
        precondition(!state.close() && !state.acceptsMicrophone)
        state.setMuted(false)
        precondition(!state.acceptsMicrophone)
        precondition(state.finalUsage == nil)
        precondition(state.receive(["type": "session.closed", "usage": ["seconds": 5]]))
        precondition(state.phase == .closed && state.finalUsage?["seconds"] as? Int == 5)
        state.fail()
        precondition(state.phase == .closed)
        precondition(!state.receive(["type": "session.output_audio.delta"]))

        var interrupted = MedousaLiveSocketLifecycle()
        precondition(interrupted.begin())
        precondition(!interrupted.close())
        precondition(interrupted.phase == .failed && interrupted.finalUsage == nil)
        precondition(!interrupted.receive(["type": "session.started", "session": ["id": "late"]]))

        var disconnected = MedousaLiveSocketLifecycle()
        precondition(disconnected.begin())
        precondition(disconnected.receive(["type": "session.started", "session": ["id": "test"]]))
        disconnected.fail()
        precondition(disconnected.phase == .failed && disconnected.finalUsage == nil)
        precondition(!disconnected.acceptsMicrophone)

        precondition(!MedousaLiveSocketLifecycle.validPCM(Data()))
        precondition(!MedousaLiveSocketLifecycle.validPCM(Data([0])))
        precondition(MedousaLiveSocketLifecycle.validPCM(Data([0, 0])))
        precondition(MedousaLiveSocketLifecycle.validPCM(Data(repeating: 0, count: 48_000)))
        precondition(!MedousaLiveSocketLifecycle.validPCM(Data(repeating: 0, count: 48_002)))
        print("Native Live lifecycle tests passed")
    }
}
