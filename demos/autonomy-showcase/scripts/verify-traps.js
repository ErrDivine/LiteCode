import { execFileSync, spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const demoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sourceWorkspace = path.join(demoRoot, 'workspace');

const traps = [
  {
    name: 'focused JavaScript repair',
    command: ['node', ['tests/orderTotals.test.js']],
    mutate(workspace) {
      const file = path.join(workspace, 'src', 'orderTotals.js');
      const source = readFileSync(file, 'utf8');
      writeFileSync(file, source.replace('item.qty * item.price', 'item.quantity * item.price'));
    }
  },
  {
    name: 'refactor fallout import',
    command: ['node', ['tests/shippingLabels.test.js']],
    mutate(workspace) {
      const oldPath = path.join(workspace, 'src', 'shippingLabels.js');
      const movedDir = path.join(workspace, 'src', 'fulfillment');
      const movedPath = path.join(movedDir, 'shippingLabels.js');
      mkdirSync(movedDir, { recursive: true });
      writeFileSync(movedPath, readFileSync(oldPath, 'utf8'));
      rmSync(oldPath, { force: true });
    }
  },
  {
    name: 'missing business rule',
    command: ['node', ['tests/discountRules.test.js']],
    mutate(workspace) {
      const file = path.join(workspace, 'src', 'discountRules.js');
      const source = readFileSync(file, 'utf8');
      writeFileSync(
        file,
        source.replace(
          "return total * 0.9;",
          "// TODO: The user paused here after adding a VIP discount test.\n    return total;"
        )
      );
    }
  },
  {
    name: 'dashboard accessibility',
    command: ['node', ['scripts/checkDashboardA11y.js']],
    mutate(workspace) {
      writeFileSync(
        path.join(workspace, 'src', 'dashboard.html'),
        `<div class="dashboard">
  <section class="metric">
    <h2>Pending orders</h2>
    <strong>24</strong>
  </section>
  <section class="metric">
    <h2>Risk holds</h2>
    <strong>3</strong>
  </section>
  <!-- TODO: User paused here before recording the operations dashboard. -->
</div>
`
      );
    }
  },
  {
    name: 'release-note readiness',
    command: ['node', ['scripts/checkReleaseNotes.js']],
    mutate(workspace) {
      writeFileSync(
        path.join(workspace, 'docs', 'release-notes.md'),
        `# Release Notes

## Autonomous Suggestions

- Marvis observes editor status and proposes focused agent work after user approval.

## Skill Routing

- Skills and toolsets are discovered from the runtime registry and used as agent identities.

<!-- TODO: Add the rollback safety section before publishing. -->
`
      );
    }
  }
];

function copyWorkspace() {
  const workspace = mkdtempSync(path.join(tmpdir(), 'marvis-autonomy-showcase-'));
  cpSync(sourceWorkspace, workspace, {
    recursive: true,
    filter(source) {
      const relative = path.relative(sourceWorkspace, source);
      return !relative.startsWith('.lite-code') && !relative.startsWith('.marvis');
    }
  });
  return workspace;
}

function runExpectSuccess(workspace, command, args) {
  execFileSync(command, args, { cwd: workspace, stdio: 'pipe' });
}

function runExpectFailure(workspace, command, args) {
  const result = spawnSync(command, args, { cwd: workspace, encoding: 'utf8' });
  if (result.status === 0) {
    throw new Error(`${command} ${args.join(' ')} unexpectedly passed`);
  }
  return `${result.stdout || ''}${result.stderr || ''}`.trim();
}

function withWorkspace(callback) {
  const workspace = copyWorkspace();
  try {
    return callback(workspace);
  } finally {
    rmSync(workspace, { recursive: true, force: true });
  }
}

withWorkspace((workspace) => {
  runExpectSuccess(workspace, 'npm', ['test']);
  console.log('baseline workspace passes');
});

for (const trap of traps) {
  withWorkspace((workspace) => {
    trap.mutate(workspace);
    const [command, args] = trap.command;
    const output = runExpectFailure(workspace, command, args);
    if (!output) {
      throw new Error(`${trap.name} failed without useful output`);
    }
    console.log(`${trap.name} trap fails as expected`);
  });
}

console.log('all autonomy showcase traps verified');

