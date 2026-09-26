import asyncio
from pathlib import Path
import shlex
import tempfile
import unittest
from unittest.mock import AsyncMock, Mock

try:
    from evals.terminal_bench.agent import MedousaCoder
    from harbor.models.agent.context import AgentContext
except ModuleNotFoundError as error:
    if not error.name.startswith("harbor"):
        raise
    MedousaCoder = None


@unittest.skipIf(MedousaCoder is None, "install evals/terminal_bench/requirements.txt")
class HarborTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        for name in ("medousa_daemon", "medousa-code", "medousa-session"):
            (self.root / name).write_bytes(b"\x7fELF")
        self.agent = MedousaCoder(logs_dir=self.root, daemon_path=str(self.root / "medousa_daemon"),
                                 model_name="provider/model", workspace="/app/work with spaces")
        self.environment = Mock()
        self.environment.exec = AsyncMock()
        self.environment.upload_file = AsyncMock()
        self.agent.exec_as_agent = AsyncMock()

    async def test_installs_and_checks_all_three_binaries(self):
        self.agent.ensure_system_dependencies = AsyncMock()
        self.agent.exec_as_root = AsyncMock()
        await self.agent.install(self.environment)
        uploads = [call.args for call in self.environment.upload_file.call_args_list]
        for name in ("medousa_daemon", "medousa-code", "medousa-session"):
            self.assertIn((self.root / name, f"/installed-agent/medousa/{name}"), uploads)
        self.assertEqual([shlex.split(call.kwargs["command"])
                          for call in self.agent.exec_as_agent.call_args_list], [
            [f"/installed-agent/medousa/{name}", "--help"]
            for name in ("medousa_daemon", "medousa-code", "medousa-session")
        ])

    async def test_explicit_sidecar_build_paths_keep_canonical_uploaded_names(self):
        code = self.root / "custom-code-build"
        session = self.root / "custom-session-build"
        (self.root / "medousa-code").rename(code)
        (self.root / "medousa-session").rename(session)
        agent = MedousaCoder(logs_dir=self.root, daemon_path=str(self.root / "medousa_daemon"),
                             code_path=str(code), session_path=str(session), model_name="provider/model")
        agent.ensure_system_dependencies = AsyncMock()
        agent.exec_as_root = AsyncMock()
        agent.exec_as_agent = AsyncMock()
        await agent.install(self.environment)
        uploads = [call.args for call in self.environment.upload_file.call_args_list]
        self.assertIn((code, "/installed-agent/medousa/medousa-code"), uploads)
        self.assertIn((session, "/installed-agent/medousa/medousa-session"), uploads)

    async def test_prompt_is_uploaded_verbatim_and_runner_uses_task_workspace(self):
        instruction = "Do the task.\n$(touch /should-not-run); 'quoted' λ\n"
        await self.agent.run(instruction, self.environment, AgentContext())
        source, destination = self.environment.upload_file.call_args.args
        self.assertEqual(source.read_text(), instruction)
        self.assertTrue(destination.endswith("instruction.txt"))
        args = shlex.split(self.agent.exec_as_agent.call_args.kwargs["command"])
        self.assertEqual(args[args.index("--workspace") + 1], "/app/work with spaces")
        self.assertNotIn(instruction, args)
        self.assertNotIn("--timeout", args)  # Harbor owns the task's timeout.
        self.assertIn("--stop", shlex.split(self.environment.exec.call_args.kwargs["command"]))

    async def test_harbor_timeout_still_stops_daemon(self):
        self.agent.exec_as_agent.side_effect = asyncio.CancelledError
        with self.assertRaises(asyncio.CancelledError):
            await self.agent.run("task", self.environment, AgentContext())
        self.environment.exec.assert_awaited_once()
        self.assertIn("--stop", self.environment.exec.call_args.kwargs["command"])

    async def test_rejects_macos_binary_before_trial(self):
        for name in ("medousa_daemon", "medousa-code", "medousa-session"):
            with self.subTest(binary=name):
                binary = self.root / name
                binary.write_bytes(b"\xcf\xfa\xed\xfe")
                with self.assertRaisesRegex(ValueError, f"{name} must be a Linux binary"):
                    MedousaCoder(logs_dir=self.root, daemon_path=str(self.root / "medousa_daemon"),
                                 model_name="provider/model")
                binary.write_bytes(b"\x7fELF")


if __name__ == "__main__":
    unittest.main()
