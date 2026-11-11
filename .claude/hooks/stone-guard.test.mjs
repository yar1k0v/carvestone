import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// STONE_GUARD_HOOK lets a run target another copy of the hook.
const hook = process.env.STONE_GUARD_HOOK ?? join(dirname(fileURLToPath(import.meta.url)), "stone-guard.mjs");

const rows = [
  ["Read of a store file", "Read", { file_path: "stone/tasks/CS-1.md" }, false],
  ["Edit of a store file", "Edit", { file_path: "stone/tasks/CS-1.md" }, true],
  ["cat of a store file", "Bash", { command: "cat stone/tasks/CS-1.md" }, false],
  ["redirect into the store", "Bash", { command: "echo x > stone/tasks/CS-1.md" }, true],
  [
    "stone through a variable, store path in text",
    "Bash",
    { command: "S=target/debug/stone.exe; $S edit CS-1 --set components=arch/a/stone/b" },
    false,
  ],
  ["cp into the store", "Bash", { command: "cp a.md stone/docs/a.md" }, true],
  ["git restore of a store file", "Bash", { command: "git restore stone/config.yml" }, false],
  ["rm in a later segment", "Bash", { command: "ls; rm -rf .stone/index" }, true],
  ["Set-Content into the store", "PowerShell", { command: "Set-Content stone/x.md 'a'" }, true],
  ["command naming no store path", "Bash", { command: "cargo check -p carvestone-core" }, false],
  [
    "Set-Content with -Path ./stone/",
    "PowerShell",
    { command: "Set-Content -Path ./stone/x.md -Value a" },
    true,
  ],
  [
    "Out-File with -FilePath .\\stone\\",
    "PowerShell",
    { command: "Out-File -FilePath .\\stone\\x.md" },
    true,
  ],
  [
    "redirect to an absolute store path",
    "Bash",
    { command: "echo x > C:/work/repo/stone/x.md" },
    true,
  ],
  ["rm of the .stone folder itself", "Bash", { command: "rm -rf .stone" }, true],
  ["rm of the stone folder itself", "Bash", { command: "rm -rf stone" }, true],
  ["Remove-Item of .stone", "PowerShell", { command: "Remove-Item -Recurse .stone" }, true],
  ["sed -i with a suffix", "Bash", { command: "sed -i.bak 's/a/b/' stone/x.md" }, true],
  ["sed -n reads the store", "Bash", { command: "sed -n '1,5p' stone/x.md" }, false],
  ["unassigned $null is not stone", "PowerShell", { command: "$null > stone/x.md" }, true],
  [
    "variable assigned in PowerShell",
    "PowerShell",
    { command: "$S = 'target/debug/stone.exe'; & $S edit CS-1 --set components=arch/a/stone/b" },
    false,
  ],
  ["rm after a pipe", "Bash", { command: "echo y | rm stone/x.md" }, true],
  ["env prefix before rm", "Bash", { command: "FOO=1 rm stone/x.md" }, true],
  [
    "tee after a pipe, target outside the store",
    "Bash",
    { command: "grep -n a stone/tasks/CS-1.md | tee docs/agent-scratch/out.txt" },
    false,
  ],
  [
    "write cmdlet reading the store in parentheses",
    "PowerShell",
    { command: "Set-Content out.txt (Get-Content stone/x.md)" },
    false,
  ],
  [
    "stone command whose text mentions a redirect into the store",
    "Bash",
    { command: 'target/debug/stone.exe edit CS-1 --text "echo x > stone/x.md"' },
    false,
  ],
  [
    "assigned variable whose text mentions a redirect into the store",
    "Bash",
    { command: 'S=target/debug/stone.exe; $S edit CS-1 --text "echo x > stone/x.md"' },
    false,
  ],
  [
    "cmdlet -Path after an option with a value",
    "PowerShell",
    { command: "Set-Content -ErrorAction Stop -Path stone/x.md -Value a" },
    true,
  ],
  ["cmdlet -Path: colon form", "PowerShell", { command: "Add-Content -Path:stone/x.md a" }, true],
  ["cmdlet -FilePath: colon form", "PowerShell", { command: "Out-File -FilePath:./stone/x.md" }, true],
  [
    "cmdlet -LiteralPath: colon form",
    "PowerShell",
    { command: "Set-Content -LiteralPath:.stone/x.md -Value a" },
    true,
  ],
  ["cmdlet -Path: colon form, store path only in the value", "PowerShell", { command: "Add-Content -Path:docs/log.txt stone/x.md" }, false],
  ["cmdlet -Value before the target", "PowerShell", { command: "Set-Content -Value a stone/x.md" }, true],
  [
    "cmdlet target is a quoted path with a space",
    "PowerShell",
    { command: 'Set-Content "my notes/stone/x.md" a' },
    true,
  ],
  [
    "cmdlet target in a parenthesised group",
    "PowerShell",
    { command: "Set-Content -Path (Join-Path $root 'stone/x.md') -Value a" },
    true,
  ],
  [
    "cmdlet target as a parenthesised first argument",
    "PowerShell",
    { command: 'Out-File -FilePath ("stone/x.md") -InputObject a' },
    true,
  ],
  [
    "store path only in the cmdlet value",
    "PowerShell",
    { command: 'Add-Content docs/agent-scratch/log.txt "see stone/x.md"' },
    false,
  ],
];

test("stone-guard denies writes to the store and nothing else", () => {
  const wrong = [];
  for (const [name, tool_name, tool_input, deny] of rows) {
    const run = spawnSync(process.execPath, [hook], {
      input: JSON.stringify({ tool_name, tool_input }),
      encoding: "utf8",
    });
    const denied = run.status === 0 && run.stdout.includes('"permissionDecision":"deny"');
    if (run.status !== 0 || denied !== deny) {
      wrong.push(`${name}: expected ${deny ? "deny" : "allow"}, got ${denied ? "deny" : "allow"} (exit ${run.status})`);
    }
  }
  assert.deepEqual(wrong, []);
});
