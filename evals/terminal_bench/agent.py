"""Harbor installed-agent protocol shim for the unmodified Medousa daemon."""

import json
from pathlib import Path, PurePosixPath
import shlex
import uuid

from harbor.agents.installed.base import BaseInstalledAgent, PackageSpec
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext


class MedousaCoder(BaseInstalledAgent):
    SYSTEM_PACKAGES = {
        **BaseInstalledAgent.SYSTEM_PACKAGES,
        "python3": PackageSpec.standard("python3"),
    }

    def __init__(self, *args, daemon_path: str, workspace: str | None = None,
                 defaults_path: str | None = None, reasoning_effort: str | None = None,
                 base_url: str | None = None, **kwargs):
        super().__init__(*args, **kwargs)
        if not self.model_name or "/" not in self.model_name:
            raise ValueError("Use --model PROVIDER/MODEL (Medousa provider id)")
        self.provider, self.model = self.model_name.split("/", 1)
        self.daemon = Path(daemon_path).resolve(strict=True)
        with self.daemon.open("rb") as binary:
            if binary.read(4) != b"\x7fELF":
                raise ValueError("daemon_path must be a Linux medousa_daemon binary")
        self.workspace = workspace
        self.defaults = Path(defaults_path).resolve(strict=True) if defaults_path else None
        self.reasoning_effort = reasoning_effort
        self.base_url = base_url
        self.remote = PurePosixPath("/installed-agent/medousa")
        self.state = PurePosixPath("/tmp/medousa-benchmark-" + uuid.uuid4().hex)

    @staticmethod
    def name() -> str:
        return "medousa-coder"

    async def install(self, environment: BaseEnvironment) -> None:
        await self.ensure_system_dependencies(environment, ("python3", "git"))
        await self.exec_as_root(environment, command=f"mkdir -p {self.remote}")
        await environment.upload_file(self.daemon, str(self.remote / "medousa_daemon"))
        await environment.upload_file(Path(__file__).with_name("runner.py"), str(self.remote / "runner.py"))
        if self.defaults:
            await environment.upload_file(self.defaults, str(self.remote / "defaults.json"))
        await self.exec_as_root(
            environment,
            command=f"chmod 755 {self.remote}/medousa_daemon; chmod 644 {self.remote}/*.py",
        )
        # Detect architecture/shared-library mismatches during setup, before a paid turn.
        await self.exec_as_agent(environment, command=f"{self.remote}/medousa_daemon --help")

    async def run(self, instruction: str, environment: BaseEnvironment, context: AgentContext) -> None:
        instruction_path = self.logs_dir / "instruction.txt"
        instruction_path.write_text(instruction, encoding="utf-8")
        await environment.upload_file(instruction_path, str(self.remote / "instruction.txt"))
        command = ["python3", str(self.remote / "runner.py"),
                   "--daemon", str(self.remote / "medousa_daemon"),
                   "--instruction", str(self.remote / "instruction.txt"),
                   "--state", str(self.state),
                   "--output", str(self.environment_logs_dir / "medousa"),
                   "--provider", self.provider, "--model", self.model]
        for flag, value in (("--workspace", self.workspace),
                            ("--reasoning-effort", self.reasoning_effort),
                            ("--base-url", self.base_url),
                            ("--defaults", str(self.remote / "defaults.json") if self.defaults else None)):
            if value is not None:
                command.extend([flag, value])
        try:
            await self.exec_as_agent(environment, command=shlex.join(command))
        finally:
            # Harbor cancels run() on its own task timeout. Stop the daemon even
            # when the environment's exec transport leaves child processes alive.
            await environment.exec(command=shlex.join([
                "python3", str(self.remote / "runner.py"), "--stop", str(self.state)
            ]), timeout_sec=10)

    def populate_context_post_run(self, context: AgentContext) -> None:
        result_path = self.logs_dir / "medousa" / "result.json"
        if result_path.exists():
            result = json.loads(result_path.read_text())
            context.metadata = {**(context.metadata or {}), "medousa_outcome": result["outcome"]}
        # The native ledger reports tool-loop usage only. Leave Harbor's total
        # token/cost fields unknown rather than labeling a partial total complete.
