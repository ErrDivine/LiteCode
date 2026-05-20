const vscode = require('vscode');
const fs = require('fs/promises');
const path = require('path');

const FILES = {
  orderTotals: path.join('src', 'orderTotals.js'),
  discountRules: path.join('src', 'discountRules.js'),
  shippingLabels: path.join('src', 'shippingLabels.js'),
  fulfillmentShippingLabels: path.join('src', 'fulfillment', 'shippingLabels.js'),
  dashboard: path.join('src', 'dashboard.html'),
  orderTest: path.join('tests', 'orderTotals.test.js'),
  discountTest: path.join('tests', 'discountRules.test.js'),
  shippingTest: path.join('tests', 'shippingLabels.test.js'),
  releaseNotes: path.join('docs', 'release-notes.md'),
  releaseCheck: path.join('scripts', 'checkReleaseNotes.js'),
  dashboardCheck: path.join('scripts', 'checkDashboardA11y.js')
};

const EXTRA_PATHS = [
  path.join('src', 'fulfillment')
];

const BASELINE = {
  [FILES.orderTotals]: `// @ts-check

/**
 * @typedef {Object} LineItem
 * @property {string} sku
 * @property {number} qty
 * @property {number} price
 * @property {boolean=} taxable
 */

/**
 * @param {LineItem[]} items
 */
export function subtotal(items) {
  return items.reduce((sum, item) => sum + item.qty * item.price, 0);
}

/**
 * @param {LineItem[]} items
 * @param {number} rate
 */
export function tax(items, rate) {
  const taxableSubtotal = items
    .filter((item) => item.taxable !== false)
    .reduce((sum, item) => sum + item.qty * item.price, 0);
  return taxableSubtotal * rate;
}

/**
 * @param {LineItem[]} items
 * @param {{ taxRate?: number, shipping?: number }} options
 */
export function invoiceTotal(items, options = {}) {
  const taxRate = options.taxRate ?? 0;
  const shipping = options.shipping ?? 0;
  return subtotal(items) + tax(items, taxRate) + shipping;
}
`,
  [FILES.discountRules]: `// @ts-check

/**
 * @param {number} total
 * @param {{ tier?: string, coupon?: string }} customer
 */
export function applyDiscount(total, customer = {}) {
  if (customer.coupon === 'SHIPFREE') {
    return Math.max(total - 5, 0);
  }
  if (customer.tier === 'vip') {
    return total * 0.9;
  }
  return total;
}
`,
  [FILES.shippingLabels]: `// @ts-check

/**
 * @param {{ id: string, priority?: 'standard' | 'express' }} order
 * @param {{ city: string, region: string }} destination
 */
export function shipmentLabel(order, destination) {
  const priority = order.priority === 'express' ? 'EXPRESS' : 'STANDARD';
  return [priority, order.id, destination.city, destination.region].join(' / ');
}
`,
  [FILES.dashboard]: `<main class="dashboard" aria-label="Operations dashboard">
  <section class="metric" aria-label="Pending orders">
    <h2>Pending orders</h2>
    <strong>24</strong>
  </section>
  <section class="metric" aria-label="Risk holds">
    <h2>Risk holds</h2>
    <strong>3</strong>
  </section>
</main>
`,
  [FILES.orderTest]: `import assert from 'node:assert/strict';
import { invoiceTotal } from '../src/orderTotals.js';

const items = [
  { sku: 'MONITOR', qty: 2, price: 100, taxable: true },
  { sku: 'CABLE', qty: 1, price: 20, taxable: false }
];

assert.equal(invoiceTotal(items, { taxRate: 0.1, shipping: 7 }), 247);
console.log('order total test passed');
`,
  [FILES.discountTest]: `import assert from 'node:assert/strict';
import { applyDiscount } from '../src/discountRules.js';

assert.equal(applyDiscount(100, { tier: 'vip' }), 90);
assert.equal(applyDiscount(8, { coupon: 'SHIPFREE' }), 3);
assert.equal(applyDiscount(4, { coupon: 'SHIPFREE' }), 0);
console.log('discount rules test passed');
`,
  [FILES.shippingTest]: `import assert from 'node:assert/strict';
import { shipmentLabel } from '../src/shippingLabels.js';

assert.equal(
  shipmentLabel(
    { id: 'SO-1042', priority: 'express' },
    { city: 'Seattle', region: 'WA' }
  ),
  'EXPRESS / SO-1042 / Seattle / WA'
);
console.log('shipping label test passed');
`,
  [FILES.releaseNotes]: `# Release Notes

## Autonomous Suggestions

- Marvis observes editor status and proposes focused agent work after user approval.

## Skill Routing

- Skills and toolsets are discovered from the runtime registry and used as agent identities.

## Rollback Safety

- Workspace writes are protected by preimage snapshots before patch tools run.
`,
  [FILES.releaseCheck]: `import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

const text = readFileSync(new URL('../docs/release-notes.md', import.meta.url), 'utf8');
for (const heading of ['Autonomous Suggestions', 'Skill Routing', 'Rollback Safety']) {
  assert.match(text, new RegExp('## ' + heading), 'missing release-note section: ' + heading);
}
console.log('release notes check passed');
`,
  [FILES.dashboardCheck]: `import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

const html = readFileSync(new URL('../src/dashboard.html', import.meta.url), 'utf8');
assert.match(html, /<main\\b[^>]*aria-label="Operations dashboard"/, 'dashboard needs a labelled main landmark');
assert.match(html, /<section\\b[^>]*aria-label="Pending orders"/, 'pending orders metric needs an accessible label');
assert.match(html, /<section\\b[^>]*aria-label="Risk holds"/, 'risk holds metric needs an accessible label');
console.log('dashboard accessibility check passed');
`
};

const TRAPS = {
  orderTotals: `// @ts-check

/**
 * @typedef {Object} LineItem
 * @property {string} sku
 * @property {number} qty
 * @property {number} price
 * @property {boolean=} taxable
 */

/**
 * @param {LineItem[]} items
 */
export function subtotal(items) {
  return items.reduce((sum, item) => sum + item.quantity * item.price, 0);
}

/**
 * @param {LineItem[]} items
 * @param {number} rate
 */
export function tax(items, rate) {
  const taxableSubtotal = items
    .filter((item) => item.taxable !== false)
    .reduce((sum, item) => sum + item.qty * item.price, 0);
  return taxableSubtotal * rate;
}

/**
 * @param {LineItem[]} items
 * @param {{ taxRate?: number, shipping?: number }} options
 */
export function invoiceTotal(items, options = {}) {
  const taxRate = options.taxRate ?? 0;
  const shipping = options.shipping ?? 0;
  return subtotal(items) + tax(items, taxRate) + shipping;
}
`,
  discountRules: `// @ts-check

/**
 * @param {number} total
 * @param {{ tier?: string, coupon?: string }} customer
 */
export function applyDiscount(total, customer = {}) {
  if (customer.coupon === 'SHIPFREE') {
    return Math.max(total - 5, 0);
  }
  // TODO: The user paused here after adding a VIP discount test.
  if (customer.tier === 'vip') {
    return total;
  }
  return total;
}
`,
  fulfillmentShippingLabels: `// @ts-check

/**
 * The user just moved this formatter into fulfillment, but the failing test
 * still imports the old public module path.
 *
 * @param {{ id: string, priority?: 'standard' | 'express' }} order
 * @param {{ city: string, region: string }} destination
 */
export function shipmentLabel(order, destination) {
  const priority = order.priority === 'express' ? 'EXPRESS' : 'STANDARD';
  return [priority, order.id, destination.city, destination.region].join(' / ');
}
`,
  dashboard: `<div class="dashboard">
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
`,
  releaseNotes: `# Release Notes

## Autonomous Suggestions

- Marvis observes editor status and proposes focused agent work after user approval.

## Skill Routing

- Skills and toolsets are discovered from the runtime registry and used as agent identities.

<!-- TODO: Add the rollback safety section before publishing. -->
`
};

let activeRun;

function activate(context) {
  const output = vscode.window.createOutputChannel('Marvis Demo');
  const controller = new DemoController(context, output);

  context.subscriptions.push(
    output,
    vscode.commands.registerCommand('marvisDemo.run', () => controller.run()),
    vscode.commands.registerCommand('marvisDemo.reset', () => controller.resetWorkspace())
  );

  if (vscode.workspace.getConfiguration('marvisDemo').get('autoRun')) {
    setTimeout(() => {
      controller.runIfDemoWorkspace().catch((error) => controller.fail(error));
    }, 2500);
  }
}

function deactivate() {}

class DemoController {
  constructor(context, output) {
    this.context = context;
    this.output = output;
  }

  async run() {
    if (!await this.isDemoWorkspace()) {
      vscode.window.showWarningMessage('Open demos/autonomy-showcase/workspace before running the Marvis demo.');
      return undefined;
    }

    if (activeRun) {
      vscode.window.showInformationMessage('Marvis demo is already running.');
      return activeRun;
    }

    activeRun = this.runInner().finally(() => {
      activeRun = undefined;
    });
    return activeRun;
  }

  async runIfDemoWorkspace() {
    if (!await this.isDemoWorkspace()) {
      return undefined;
    }
    return this.run();
  }

  async runInner() {
    this.output.show(true);
    this.log('Starting Marvis autonomy showcase.');

    await this.resetWorkspace(false);
    await this.ensureMarvisStarted();

    const delay = Math.max(
      7000,
      Number(vscode.workspace.getConfiguration('marvisDemo').get('stageDelayMs') || 25000)
    );

    await this.runTrapStage(delay, 'Focused JavaScript repair trap', 'Waiting for Marvis to infer the focused repair.', async () => {
      await this.replaceDemoFile(FILES.orderTotals, TRAPS.orderTotals);
      await this.focusFile(FILES.orderTotals, 15, 50);
      await this.runTask('Marvis demo: order total test trap');
    });

    await this.runTrapStage(delay, 'Refactor fallout import trap', 'Waiting for Marvis to connect the moved file with the failing import.', async () => {
      await this.removeDemoPath(FILES.shippingLabels);
      await this.replaceDemoFile(FILES.fulfillmentShippingLabels, TRAPS.fulfillmentShippingLabels);
      await this.focusFile(FILES.shippingTest, 2, 38);
      await this.runTask('Marvis demo: shipping label refactor trap');
    });

    await this.runTrapStage(delay, 'Missing business rule trap', 'Waiting for Marvis to pair the failing test with the cursor TODO.', async () => {
      await this.replaceDemoFile(FILES.discountRules, TRAPS.discountRules);
      await this.focusFile(FILES.discountRules, 11, 5);
      await this.runTask('Marvis demo: discount rule test trap');
    });

    await this.runTrapStage(delay, 'Frontend readiness trap', 'Waiting for Marvis to route the accessible UI patch.', async () => {
      await this.replaceDemoFile(FILES.dashboard, TRAPS.dashboard);
      await this.focusFile(FILES.dashboard, 9, 7);
      await this.runTask('Marvis demo: dashboard accessibility trap');
    });

    await this.runTrapStage(delay, 'Documentation readiness trap', 'Waiting for Marvis to route the documentation follow-up.', async () => {
      await this.replaceDemoFile(FILES.releaseNotes, TRAPS.releaseNotes);
      await this.focusFile(FILES.releaseNotes, 11, 10);
      await this.runTask('Marvis demo: release notes check trap');
    });

    await this.stage('Reset demo workspace', async () => {
      await this.resetWorkspace(false);
      await this.focusFile('README.md', 1, 1);
    });

    this.log('Demo complete. Workspace restored to baseline.');
    vscode.window.showInformationMessage('Marvis autonomy showcase complete.');
  }

  async runTrapStage(delay, title, waitMessage, callback) {
    await this.resetWorkspace(false);
    await this.stage(title, callback);
    await this.waitForMarvis(delay, waitMessage);
  }

  async resetWorkspace(showMessage = true) {
    if (!await this.isDemoWorkspace()) {
      vscode.window.showWarningMessage('Open demos/autonomy-showcase/workspace before resetting the Marvis demo.');
      return;
    }

    const root = this.workspaceRoot();
    await fs.mkdir(path.join(root, 'src'), { recursive: true });
    await fs.mkdir(path.join(root, 'tests'), { recursive: true });
    await fs.mkdir(path.join(root, 'scripts'), { recursive: true });
    await fs.mkdir(path.join(root, 'docs'), { recursive: true });

    for (const relativePath of EXTRA_PATHS) {
      await this.removeDemoPath(relativePath);
    }

    for (const [relativePath, contents] of Object.entries(BASELINE)) {
      await this.replaceDemoFile(relativePath, contents, { reveal: false });
    }

    if (showMessage) {
      this.log('Workspace reset to baseline.');
      vscode.window.showInformationMessage('Marvis demo workspace reset.');
    }
  }

  async ensureMarvisStarted() {
    try {
      await vscode.commands.executeCommand('marvis.showStatus');
      await vscode.commands.executeCommand('marvis.start');
      this.log('Marvis started. Status panel requested.');
    } catch (error) {
      const message = error && error.message ? error.message : String(error);
      throw new Error(`Unable to start Marvis. Open the demo with the Marvis extension loaded. ${message}`);
    }
  }

  async stage(title, callback) {
    this.log('');
    this.log(`== ${title} ==`);
    vscode.window.setStatusBarMessage(`Marvis demo: ${title}`, 5000);
    await callback();
  }

  async waitForMarvis(ms, message) {
    this.log(message);
    await sleep(ms);
  }

  async replaceDemoFile(relativePath, contents, options = {}) {
    const root = this.workspaceRoot();
    const uri = vscode.Uri.file(path.join(root, relativePath));
    await fs.mkdir(path.dirname(uri.fsPath), { recursive: true });
    try {
      await fs.access(uri.fsPath);
    } catch (_error) {
      await fs.writeFile(uri.fsPath, '', 'utf8');
    }
    const openDocument = vscode.workspace.textDocuments.find((document) => document.uri.fsPath === uri.fsPath);
    const document = openDocument || await vscode.workspace.openTextDocument(uri);
    if (options.reveal !== false && !openDocument) {
      await vscode.window.showTextDocument(document, { preview: false, viewColumn: vscode.ViewColumn.One });
    }
    const fullRange = new vscode.Range(
      document.positionAt(0),
      document.positionAt(document.getText().length)
    );
    const edit = new vscode.WorkspaceEdit();
    edit.replace(uri, fullRange, contents);
    const applied = await vscode.workspace.applyEdit(edit);
    if (!applied) {
      throw new Error(`VSCode refused edit for ${relativePath}`);
    }
    await document.save();
    this.log(`Wrote ${relativePath}`);
  }

  async removeDemoPath(relativePath) {
    const root = this.workspaceRoot();
    const target = path.join(root, relativePath);
    await fs.rm(target, { recursive: true, force: true });
    this.log(`Removed ${relativePath}`);
  }

  async focusFile(relativePath, oneBasedLine, oneBasedColumn) {
    const root = this.workspaceRoot();
    const uri = vscode.Uri.file(path.join(root, relativePath));
    const document = await vscode.workspace.openTextDocument(uri);
    const editor = await vscode.window.showTextDocument(document, { preview: false, viewColumn: vscode.ViewColumn.One });
    const line = Math.min(Math.max(0, oneBasedLine - 1), Math.max(0, document.lineCount - 1));
    const character = Math.min(
      Math.max(0, oneBasedColumn - 1),
      document.lineAt(line).range.end.character
    );
    const position = new vscode.Position(line, character);
    editor.selection = new vscode.Selection(position, position);
    editor.revealRange(new vscode.Range(position, position), vscode.TextEditorRevealType.InCenter);
    this.log(`Focused ${relativePath}:${oneBasedLine}:${oneBasedColumn}`);
  }

  async runTask(taskName) {
    const tasks = await vscode.tasks.fetchTasks();
    const task = tasks.find((candidate) => candidate.name === taskName);
    if (!task) {
      throw new Error(`Demo task not found: ${taskName}`);
    }
    this.log(`Running VSCode task: ${taskName}`);
    await new Promise((resolve, reject) => {
      let subscription;
      const timeout = setTimeout(() => {
        if (subscription) {
          subscription.dispose();
        }
        reject(new Error(`Timed out waiting for task: ${taskName}`));
      }, 30000);
      subscription = vscode.tasks.onDidEndTaskProcess((event) => {
        if (event.execution.task.name !== taskName) {
          return;
        }
        clearTimeout(timeout);
        subscription.dispose();
        this.log(`Task ended with exit code ${event.exitCode}: ${taskName}`);
        resolve();
      });
      vscode.tasks.executeTask(task).then(undefined, (error) => {
        clearTimeout(timeout);
        subscription.dispose();
        reject(error);
      });
    });
  }

  workspaceRoot() {
    const folder = vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders[0];
    if (!folder) {
      throw new Error('Open the Marvis demo workspace folder first.');
    }
    return folder.uri.fsPath;
  }

  async isDemoWorkspace() {
    try {
      const root = this.workspaceRoot();
      await fs.access(path.join(root, '.marvis-demo.json'));
      return true;
    } catch (_error) {
      return false;
    }
  }

  log(message) {
    this.output.appendLine(message);
  }

  fail(error) {
    const message = error && error.message ? error.message : String(error);
    this.output.appendLine(`[error] ${message}`);
    vscode.window.showErrorMessage(`Marvis demo failed: ${message}`);
  }
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

module.exports = { activate, deactivate };
