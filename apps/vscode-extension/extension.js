const vscode = require('vscode');
const childProcess = require('child_process');
const fs = require('fs');
const path = require('path');

let activeController;

function activate(context) {
  const output = vscode.window.createOutputChannel('Marvis');
  const controller = new MarvisController(context, output);
  activeController = controller;
  controller.watchWorkspace();

  context.subscriptions.push(
    output,
    controller,
    vscode.commands.registerCommand('marvis.start', () => controller.start()),
    vscode.commands.registerCommand('marvis.showStatus', () => controller.showStatus()),
    vscode.commands.registerCommand('marvis.refreshStatus', () => controller.refreshStatus()),
    vscode.commands.registerCommand('marvis.checkAutonomyNow', () => controller.checkAutonomyNow()),
    vscode.commands.registerCommand('marvis.ask', () => controller.ask()),
    vscode.commands.registerCommand('marvis.askAboutSelection', () => controller.askAboutSelection()),
    vscode.commands.registerCommand('marvis.fixNearCursor', () => controller.fixNearCursor()),
    vscode.commands.registerCommand('marvis.explainDiagnostic', (uri, diagnostic) =>
      controller.explainDiagnostic(uri, diagnostic)
    ),
    vscode.commands.registerCommand('marvis.recordTerminalFailure', () =>
      controller.recordTerminalFailure()
    ),
    vscode.commands.registerCommand('marvis.runCommandAndRecord', () =>
      controller.runCommandAndRecord()
    ),
    vscode.commands.registerCommand('marvis.runTaskAndRecord', () => controller.runTaskAndRecord()),
    vscode.languages.registerCodeActionsProvider(
      { scheme: 'file' },
      new MarvisCodeActionProvider(),
      { providedCodeActionKinds: [vscode.CodeActionKind.QuickFix] }
    )
  );
}

function deactivate() {
  if (activeController) {
    activeController.dispose();
  }
}

class MarvisController {
  constructor(context, output) {
    this.context = context;
    this.output = output;
    this.client = undefined;
    this.panel = undefined;
    this.lastReport = undefined;
    this.lastStatus = undefined;
    this.lastAutonomyDecision = undefined;
    this.activeSuggestion = undefined;
    this.statusTimer = undefined;
    this.autonomyTimer = undefined;
    this.heartbeatTimer = undefined;
    this.autonomyInFlight = false;
    this.recentlyOpened = [];
    this.recentlySaved = [];
    this.commandResults = [];
    this.runningTasks = new Map();
    this.debugSessions = new Map();
    this.agentLog = [];
    this.processes = new Map();
  }

  dispose() {
    if (this.statusTimer) {
      clearTimeout(this.statusTimer);
      this.statusTimer = undefined;
    }
    if (this.autonomyTimer) {
      clearTimeout(this.autonomyTimer);
      this.autonomyTimer = undefined;
    }
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = undefined;
    }
    if (this.client) {
      this.client.dispose();
      this.client = undefined;
    }
    if (this.panel) {
      this.panel.dispose();
      this.panel = undefined;
    }
  }

  watchWorkspace() {
    this.context.subscriptions.push(
      vscode.window.onDidChangeActiveTextEditor((editor) => {
        if (editor && editor.document.uri.scheme === 'file') {
          this.addRecent(this.recentlyOpened, editor.document.uri.fsPath);
        }
        this.scheduleStatusRefresh('active editor changed', 'status_change');
      }),
      vscode.window.onDidChangeTextEditorSelection(() =>
        this.scheduleStatusRefresh('selection changed', 'idle')
      ),
      vscode.window.onDidChangeVisibleTextEditors(() =>
        this.scheduleStatusRefresh('visible editors changed', 'status_change')
      ),
      vscode.languages.onDidChangeDiagnostics(() =>
        this.scheduleStatusRefresh('diagnostics changed', 'diagnostics_changed')
      ),
      vscode.workspace.onDidSaveTextDocument((document) => {
        if (document.uri.scheme === 'file') {
          this.addRecent(this.recentlySaved, document.uri.fsPath);
        }
        this.scheduleStatusRefresh('file saved', 'file_saved');
      }),
      vscode.tasks.onDidStartTask((event) => {
        const name = event.execution.task.name;
        this.runningTasks.set(name, {
          name,
          kind: taskKind(event.execution.task),
          is_running: true
        });
        this.scheduleStatusRefresh('task started', 'status_change');
      }),
      vscode.tasks.onDidEndTaskProcess((event) => {
        const name = event.execution.task.name;
        const task = {
          name,
          kind: taskKind(event.execution.task),
          is_running: false,
          last_exit_code: event.exitCode
        };
        this.runningTasks.set(name, task);
        this.sendCommandResult({
          command: `vscode task: ${name}`,
          cwd: this.workspaceRoot(),
          output_tail: `VSCode task ended with exit code ${event.exitCode}.`,
          exit_code: typeof event.exitCode === 'number' ? event.exitCode : undefined,
          timestamp_ms: Date.now()
        });
        this.scheduleStatusRefresh('task ended', 'task_ended');
      }),
      vscode.debug.onDidStartDebugSession((session) => {
        this.debugSessions.set(session.id, {
          name: session.name,
          kind: session.type,
          state: 'running'
        });
        this.scheduleStatusRefresh('debug session started', 'status_change');
      }),
      vscode.debug.onDidTerminateDebugSession((session) => {
        this.debugSessions.set(session.id, {
          name: session.name,
          kind: session.type,
          state: 'terminated'
        });
        this.scheduleStatusRefresh('debug session terminated', 'debug_terminated');
      })
    );
    if (typeof vscode.window.onDidEndTerminalShellExecution === 'function') {
      this.context.subscriptions.push(
        vscode.window.onDidEndTerminalShellExecution(async (event) => {
          const execution = event.execution;
          const commandLine = execution && execution.commandLine;
          const command = commandLine && (commandLine.value || commandLine);
          if (!command) {
            return;
          }
          let output = '';
          if (typeof execution.read === 'function') {
            try {
              for await (const chunk of execution.read()) {
                output += String(chunk);
                if (output.length > 20000) {
                  output = truncateTail(output, 12000);
                }
              }
            } catch (error) {
              output += `\nUnable to read terminal output: ${error.message}`;
            }
          }
          await this.sendCommandResult({
            command: String(command),
            cwd: this.workspaceRoot(),
            output_tail: truncateTail(output || 'Terminal command ended.', 12000),
            exit_code: typeof event.exitCode === 'number' ? event.exitCode : undefined,
            timestamp_ms: Date.now()
          });
        })
      );
    }
  }

  async start() {
    await this.ensureClient();
    await this.refreshStatus();
    this.scheduleAutonomyTick('manual');
    this.showStatus();
  }

  async showStatus() {
    await this.ensureClient();
    if (!this.panel) {
      this.panel = vscode.window.createWebviewPanel(
        'marvisStatus',
        'Marvis',
        vscode.ViewColumn.Beside,
        { enableScripts: true, retainContextWhenHidden: true }
      );
      this.panel.webview.html = renderPanelHtml();
      this.panel.webview.onDidReceiveMessage((message) => this.handlePanelMessage(message));
      this.panel.onDidDispose(() => {
        this.panel = undefined;
      });
    }
    this.updatePanel();
  }

  async refreshStatus() {
    await this.ensureClient();
    const status = await this.collectStatus();
    this.lastStatus = status;
    const response = await this.client.request(
      'status_update',
      { status },
      ['status_report', 'error'],
      30000
    );
    if (response.report) {
      this.lastReport = response.report;
      this.updatePanel();
    }
    this.scheduleAutonomyTick('command_result');
  }

  async checkAutonomyNow() {
    await this.ensureClient();
    await this.runAutonomyTick('manual');
  }

  startAutonomyHeartbeat() {
    const config = vscode.workspace.getConfiguration('marvis');
    if (!config.get('autonomy.enabled')) {
      if (this.heartbeatTimer) {
        clearInterval(this.heartbeatTimer);
        this.heartbeatTimer = undefined;
      }
      return;
    }
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
    }
    const interval = Math.max(5000, Number(config.get('autonomy.heartbeatIntervalMs') || 30000));
    this.heartbeatTimer = setInterval(() => {
      this.runAutonomyTick('heartbeat').catch((error) => {
        this.output.appendLine(`autonomy heartbeat failed: ${error.message}`);
      });
    }, interval);
  }

  scheduleAutonomyTick(trigger) {
    const config = vscode.workspace.getConfiguration('marvis');
    if (!config.get('autonomy.enabled')) {
      return;
    }
    if (!this.client || !this.client.isRunning() || this.hasRunningProcess()) {
      return;
    }
    if (this.autonomyTimer) {
      clearTimeout(this.autonomyTimer);
    }
    const delay = Math.max(500, Number(config.get('autonomy.idleDelayMs') || 3000));
    this.autonomyTimer = setTimeout(() => {
      this.autonomyTimer = undefined;
      this.runAutonomyTick(trigger || 'idle').catch((error) => {
        this.output.appendLine(`autonomy tick failed: ${error.message}`);
      });
    }, delay);
  }

  async runAutonomyTick(trigger) {
    const config = vscode.workspace.getConfiguration('marvis');
    if (!config.get('autonomy.enabled') && trigger !== 'manual') {
      return;
    }
    if (!this.client || !this.client.isRunning() || this.autonomyInFlight || this.hasRunningProcess()) {
      return;
    }
    this.autonomyInFlight = true;
    try {
      const status = await this.collectStatus();
      this.lastStatus = status;
      await this.client.request(
        'autonomy_tick',
        { status, trigger },
        ['autonomy_decision', 'error'],
        120000
      );
    } finally {
      this.autonomyInFlight = false;
    }
  }

  handleAutonomyDecision(decision) {
    if (!decision) {
      return;
    }
    this.lastAutonomyDecision = decision;
    if (decision.type === 'suggest' && decision.suggestion) {
      this.activeSuggestion = decision.suggestion;
      const route = decision.suggestion.route || {};
      const task = route.task || {};
      const agent = route.agent || {};
      this.addLog('suggestion', `${task.title || 'Suggested task'} -> ${agent.label || agent.id || 'agent'}`);
      this.showSuggestionNotification(decision.suggestion);
    } else if (decision.type === 'suppressed') {
      this.activeSuggestion = undefined;
    }
    this.updatePanel();
  }

  async showSuggestionNotification(suggestion) {
    const route = suggestion.route || {};
    const task = route.task || {};
    const agent = route.agent || {};
    const title = task.title || 'Marvis found a possible task';
    const choice = await vscode.window.showInformationMessage(
      `${title} (${agent.label || agent.id || 'agent'}, ${approvalLabel(suggestion.required_approval)})`,
      'Accept',
      'Dismiss'
    );
    if (choice === 'Accept') {
      await this.runSuggestedTask(suggestion.suggestion_id);
    } else if (choice === 'Dismiss') {
      await this.dismissSuggestion(suggestion.suggestion_id);
    }
  }

  async runSuggestedTask(suggestionId) {
    const suggestion = this.activeSuggestion && this.activeSuggestion.suggestion_id === suggestionId
      ? this.activeSuggestion
      : undefined;
    const approval = suggestion ? suggestion.required_approval : readOnlyApproval();
    await this.ensureClient();
    await this.showStatus();
    try {
      await this.client.request(
        'run_suggested_task',
        { suggestion_id: suggestionId, approval },
        ['complete', 'error'],
        20 * 60 * 1000
      );
      this.activeSuggestion = undefined;
      this.updatePanel();
    } catch (error) {
      this.addLog('error', error.message);
      vscode.window.showErrorMessage(`Marvis failed: ${error.message}`);
    }
  }

  async dismissSuggestion(suggestionId) {
    if (!suggestionId) {
      return;
    }
    await this.ensureClient();
    try {
      await this.client.request(
        'dismiss_suggestion',
        { suggestion_id: suggestionId },
        ['autonomy_decision', 'error'],
        30000
      );
    } catch (error) {
      this.addLog('error', error.message);
    }
  }

  async ask() {
    const prompt = await vscode.window.showInputBox({
      title: 'Ask Marvis',
      prompt: 'What should Marvis do with the current VSCode context?'
    });
    if (!prompt) {
      return;
    }
    const approval = await this.pickApproval('Ask Marvis');
    if (!approval) {
      return;
    }
    await this.runPrompt(prompt, approval);
  }

  async askAboutSelection() {
    const editor = vscode.window.activeTextEditor;
    const selectionText = editor ? selectedText(editor) : '';
    const prompt = selectionText
      ? `Explain this selected code and point out risks or likely next edits:\n\n${truncate(selectionText, 4000)}`
      : 'Explain the code near my cursor and how it fits the current file.';
    await this.runPrompt(prompt, readOnlyApproval());
  }

  async fixNearCursor() {
    const choice = await vscode.window.showWarningMessage(
      'Marvis will inspect the focused code and may use runtime tools to edit files after the model decides a patch is needed.',
      { modal: true },
      'Continue'
    );
    if (choice !== 'Continue') {
      return;
    }
    await this.runPrompt(
      'Fix the issue near my cursor. Use the active editor, cursor bubble, diagnostics, terminal/task failures, and git state. Keep the patch small and run targeted verification if possible.',
      patchApproval()
    );
  }

  async explainDiagnostic(uri, diagnostic) {
    let targetUri = uri;
    let targetDiagnostic = diagnostic;
    if (!targetDiagnostic) {
      const editor = vscode.window.activeTextEditor;
      if (!editor) {
        return;
      }
      targetUri = editor.document.uri;
      const diagnostics = vscode.languages.getDiagnostics(targetUri);
      targetDiagnostic = diagnostics[0];
    }
    if (!targetDiagnostic) {
      vscode.window.showInformationMessage('No diagnostic found for the current file.');
      return;
    }
    const file = targetUri && targetUri.fsPath ? targetUri.fsPath : 'current file';
    const approval = await this.pickApproval('Explain Diagnostic');
    if (!approval) {
      return;
    }
    await this.runPrompt(
      `Explain this diagnostic and propose the smallest safe fix.\nFile: ${file}\nMessage: ${targetDiagnostic.message}\nRange: ${rangeLabel(targetDiagnostic.range)}`,
      approval
    );
  }

  async recordTerminalFailure() {
    const command = await vscode.window.showInputBox({
      title: 'Record Terminal Failure',
      prompt: 'Command that failed',
      value: 'cargo test'
    });
    if (!command) {
      return;
    }
    const outputTail = await vscode.window.showInputBox({
      title: 'Record Terminal Failure',
      prompt: 'Paste the important output tail',
      value: ''
    });
    await this.sendCommandResult({
      command,
      cwd: this.workspaceRoot(),
      output_tail: outputTail || 'Failure recorded from VSCode terminal.',
      exit_code: 1,
      timestamp_ms: Date.now()
    });
  }

  async runCommandAndRecord() {
    const command = await vscode.window.showInputBox({
      title: 'Run Command and Record Result',
      prompt: 'Command to run from the workspace root',
      value: 'cargo test'
    });
    if (!command) {
      return;
    }
    const result = await this.runLocalCommand(command);
    await this.sendCommandResult(result);
  }

  async runTaskAndRecord() {
    const tasks = await vscode.tasks.fetchTasks();
    if (!tasks.length) {
      vscode.window.showInformationMessage('No VSCode tasks were found.');
      return;
    }
    const picked = await vscode.window.showQuickPick(
      tasks.map((task) => ({ label: task.name, task })),
      { title: 'Run VSCode Task and Record Result' }
    );
    if (!picked) {
      return;
    }
    const execution = await vscode.tasks.executeTask(picked.task);
    const exitCode = await new Promise((resolve) => {
      const disposable = vscode.tasks.onDidEndTaskProcess((event) => {
        if (event.execution === execution) {
          disposable.dispose();
          resolve(event.exitCode);
        }
      });
    });
    await this.sendCommandResult({
      command: `vscode task: ${picked.task.name}`,
      cwd: this.workspaceRoot(),
      output_tail: `VSCode task ended with exit code ${exitCode}.`,
      exit_code: typeof exitCode === 'number' ? exitCode : undefined,
      timestamp_ms: Date.now()
    });
  }

  async runPrompt(prompt, approval) {
    await this.ensureClient();
    await this.showStatus();
    this.addLog('user', prompt);
    const status = await this.collectStatus();
    this.lastStatus = status;
    this.updatePanel();
    try {
      await this.client.request(
        'user_prompt',
        { prompt, status, approval: approval || readOnlyApproval() },
        ['complete', 'error'],
        20 * 60 * 1000
      );
    } catch (error) {
      this.addLog('error', error.message);
      vscode.window.showErrorMessage(`Marvis failed: ${error.message}`);
    }
  }

  async sendCommandResult(result) {
    this.commandResults.push(result);
    if (this.commandResults.length > 20) {
      this.commandResults.shift();
    }
    if (!this.client || !this.client.isRunning()) {
      this.scheduleStatusRefresh('command result recorded', 'command_result');
      return;
    }
    const response = await this.client.request(
      'command_result',
      { result },
      ['status_report', 'error'],
      30000
    );
    if (response.report) {
      this.lastReport = response.report;
      this.updatePanel();
    }
  }

  async ensureClient() {
    if (this.client && this.client.isRunning()) {
      return;
    }
    const runtime = resolveRuntime(this.context, this.workspaceRoot());
    this.output.appendLine(`Starting Marvis runtime: ${runtime.command} ${runtime.args.join(' ')}`);
    this.client = new RuntimeClient(runtime, this.output, (message) => this.handleRuntimeMessage(message));
    await this.client.start();

    const config = vscode.workspace.getConfiguration('marvis');
    const response = await this.client.request(
      'initialize',
      {
        workspace_root: this.workspaceRoot(),
        model: config.get('model'),
        api_key: String(config.get('apiKey') || '') || undefined,
        base_url: String(config.get('baseUrl') || '') || undefined,
        thinking_mode: String(config.get('thinkingMode') || 'auto'),
        reasoning_effort: String(config.get('reasoningEffort') || '') || undefined,
        max_tokens: config.get('maxTokens')
      },
      ['ready', 'error'],
      120000
    );
    if (response.report) {
      this.lastReport = response.report;
    }
    this.startAutonomyHeartbeat();
    this.updatePanel();
  }

  handleRuntimeMessage(message) {
    if (message.type === 'ready' || message.type === 'status_report' || message.type === 'complete') {
      if (message.report) {
        this.lastReport = message.report;
        this.updatePanel();
      }
      return;
    }
    if (message.type === 'agent_event') {
      this.handleAgentEvent(message.event);
      return;
    }
    if (message.type === 'process_update' && message.process) {
      this.processes.set(message.process.process_id, message.process);
      this.updatePanel();
      return;
    }
    if (message.type === 'autonomy_decision') {
      this.handleAutonomyDecision(message.decision);
      return;
    }
    if (message.type === 'error') {
      this.addLog('error', message.message);
      this.updatePanel();
    }
  }

  handleAgentEvent(event) {
    if (!event) {
      return;
    }
    switch (event.type) {
      case 'delta':
        this.addLog('assistant_delta', event.text);
        this.output.append(event.text);
        break;
      case 'agent_message':
        this.addLog('assistant', event.text);
        this.output.appendLine('');
        break;
      case 'tool_start':
        this.addLog('tool', `start ${event.name} ${event.arguments || ''}`);
        this.output.appendLine(`[tool] ${event.name} ${event.arguments || ''}`);
        break;
      case 'tool_end':
        this.addLog('tool', `done ${event.name}`);
        this.output.appendLine(`[tool] ${event.name} done\n${truncate(event.output || '', 4000)}`);
        break;
      case 'turn_started':
        this.addLog('state', `turn started ${event.turn_id}`);
        break;
      case 'turn_complete':
        this.addLog('state', `turn complete ${event.turn_id}`);
        break;
      case 'error':
        this.addLog('error', event.message);
        break;
      default:
        break;
    }
    this.updatePanel();
  }

  async collectStatus() {
    const activeEditor = vscode.window.activeTextEditor;
    const activeEditorRef = editorRef(activeEditor);
    const cursorContext = activeEditor ? cursorContextFor(activeEditor) : undefined;
    const openEditors = collectOpenEditors();
    const visibleRanges = vscode.window.visibleTextEditors
      .filter((editor) => editor.document.uri.scheme === 'file')
      .flatMap((editor) =>
        editor.visibleRanges.map((range) => ({
          path: editor.document.uri.fsPath,
          range: textRange(range)
        }))
      );
    const selections = activeEditor && activeEditor.document.uri.scheme === 'file'
      ? activeEditor.selections.map((selection) => ({
          path: activeEditor.document.uri.fsPath,
          anchor: position(selection.anchor),
          active: position(selection.active),
          is_reversed: selection.isReversed,
          selected_text: selection.isEmpty
            ? undefined
            : truncate(activeEditor.document.getText(selection), 4000)
        }))
      : [];

    return {
      active_editor: activeEditorRef,
      open_editors: openEditors,
      visible_ranges: visibleRanges,
      selections,
      cursor_context: cursorContext,
      recently_opened_files: this.recentlyOpened.slice(0, 20),
      recently_saved_files: this.recentlySaved.slice(0, 20),
      problems: collectDiagnostics(this.workspaceRoot()),
      terminal_sessions: this.commandResults.slice(-5).map((result, index) => ({
        id: `marvis-command-${index}`,
        name: 'Marvis Recorded Command',
        cwd: result.cwd,
        last_command: result.command,
        last_output_tail: result.output_tail,
        last_exit_code: result.exit_code
      })),
      running_tasks: Array.from(this.runningTasks.values()),
      debug_sessions: Array.from(this.debugSessions.values()),
      workspace_trusted: vscode.workspace.isTrusted,
      remote_name: vscode.env.remoteName,
      profile_name: vscode.env.appName
    };
  }

  scheduleStatusRefresh(_reason, autonomyTrigger) {
    if (!this.client || !this.client.isRunning()) {
      return;
    }
    if (this.statusTimer) {
      clearTimeout(this.statusTimer);
    }
    this.statusTimer = setTimeout(() => {
      this.statusTimer = undefined;
      this.refreshStatus().catch((error) => {
        this.output.appendLine(`status refresh failed: ${error.message}`);
      });
      this.scheduleAutonomyTick(autonomyTrigger || 'status_change');
    }, 500);
  }

  async runLocalCommand(command) {
    await this.showStatus();
    const cwd = this.workspaceRoot();
    this.output.show(true);
    this.output.appendLine(`$ ${command}`);
    const started = Date.now();
    return new Promise((resolve) => {
      const child = childProcess.spawn(command, {
        cwd,
        shell: true,
        env: process.env
      });
      let output = '';
      child.stdout.on('data', (chunk) => {
        const text = chunk.toString();
        output += text;
        this.output.append(text);
      });
      child.stderr.on('data', (chunk) => {
        const text = chunk.toString();
        output += text;
        this.output.append(text);
      });
      child.on('error', (error) => {
        output += error.message;
      });
      child.on('close', (code) => {
        this.output.appendLine(`\n[exit ${code}] ${Date.now() - started}ms`);
        resolve({
          command,
          cwd,
          output_tail: truncateTail(output, 12000),
          exit_code: typeof code === 'number' ? code : undefined,
          timestamp_ms: Date.now()
        });
      });
    });
  }

  addRecent(list, file) {
    const existing = list.indexOf(file);
    if (existing >= 0) {
      list.splice(existing, 1);
    }
    list.unshift(file);
    if (list.length > 20) {
      list.pop();
    }
  }

  addLog(kind, text) {
    if (!text) {
      return;
    }
    const last = this.agentLog[this.agentLog.length - 1];
    if (kind === 'assistant_delta' && last && last.kind === 'assistant_delta') {
      last.text += text;
    } else {
      this.agentLog.push({ kind, text, at: new Date().toLocaleTimeString() });
    }
    if (this.agentLog.length > 120) {
      this.agentLog.splice(0, this.agentLog.length - 120);
    }
  }

  updatePanel() {
    if (!this.panel) {
      return;
    }
    this.panel.webview.postMessage({
      type: 'state',
      report: this.lastReport,
      status: this.lastStatus,
      log: this.agentLog,
      processes: Array.from(this.processes.values()).slice(-20),
      autonomyDecision: this.lastAutonomyDecision,
      suggestion: this.activeSuggestion
    });
  }

  hasRunningProcess() {
    return Array.from(this.processes.values()).some((process) =>
      ['created', 'running_model_turn', 'running_tool_call'].includes(process.state)
    );
  }

  async pickApproval(title) {
    const picked = await vscode.window.showQuickPick(
      [
        {
          label: 'Read only',
          description: 'Use editor status and read/search tools only.',
          approval: readOnlyApproval()
        },
        {
          label: 'Patch and checks',
          description: 'Allow workspace edits and safe local check/test commands.',
          approval: patchApproval()
        },
        {
          label: 'Risky shell',
          description: 'Allow broader shell/git/network-like commands for this prompt.',
          approval: riskyApproval()
        }
      ],
      { title }
    );
    return picked && picked.approval;
  }

  async handlePanelMessage(message) {
    switch (message.command) {
      case 'ask':
        await this.ask();
        break;
      case 'refresh':
        await this.refreshStatus();
        break;
      case 'runCommand':
        await this.runCommandAndRecord();
        break;
      case 'recordFailure':
        await this.recordTerminalFailure();
        break;
      case 'acceptSuggestion':
        await this.runSuggestedTask(message.suggestionId);
        break;
      case 'dismissSuggestion':
        await this.dismissSuggestion(message.suggestionId);
        break;
      case 'checkAutonomy':
        await this.checkAutonomyNow();
        break;
      default:
        break;
    }
  }

  workspaceRoot() {
    const folder = vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders[0];
    return folder ? folder.uri.fsPath : process.cwd();
  }
}

class RuntimeClient {
  constructor(runtime, output, onMessage) {
    this.runtime = runtime;
    this.output = output;
    this.onMessage = onMessage;
    this.process = undefined;
    this.nextId = 1;
    this.pending = new Map();
    this.buffer = '';
  }

  async start() {
    if (this.process) {
      return;
    }
    this.process = childProcess.spawn(this.runtime.command, this.runtime.args, {
      cwd: this.runtime.cwd,
      env: process.env,
      stdio: ['pipe', 'pipe', 'pipe']
    });

    this.process.stdout.on('data', (chunk) => this.handleStdout(chunk.toString()));
    this.process.stderr.on('data', (chunk) => this.output.append(chunk.toString()));
    this.process.on('error', (error) => {
      this.rejectAll(error);
    });
    this.process.on('exit', (code, signal) => {
      const error = new Error(`Marvis runtime exited with code ${code} signal ${signal || ''}`);
      this.rejectAll(error);
      this.process = undefined;
    });
  }

  isRunning() {
    return Boolean(this.process && !this.process.killed);
  }

  request(type, payload, terminalTypes, timeoutMs) {
    if (!this.process || !this.process.stdin.writable) {
      return Promise.reject(new Error('Marvis runtime is not running'));
    }
    const id = this.nextId++;
    const message = Object.assign({ id, type }, payload || {});
    const encoded = `${JSON.stringify(message)}\n`;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`Marvis request timed out: ${type}`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, terminalTypes, timer });
      this.process.stdin.write(encoded);
    });
  }

  handleStdout(text) {
    this.buffer += text;
    while (true) {
      const newline = this.buffer.indexOf('\n');
      if (newline < 0) {
        break;
      }
      const line = this.buffer.slice(0, newline).trim();
      this.buffer = this.buffer.slice(newline + 1);
      if (!line) {
        continue;
      }
      let message;
      try {
        message = JSON.parse(line);
      } catch (error) {
        this.output.appendLine(`invalid Marvis JSON: ${line}`);
        continue;
      }
      this.onMessage(message);
      const pending = typeof message.id === 'number' ? this.pending.get(message.id) : undefined;
      if (pending && pending.terminalTypes.includes(message.type)) {
        clearTimeout(pending.timer);
        this.pending.delete(message.id);
        if (message.type === 'error') {
          pending.reject(new Error(message.message || 'Marvis request failed'));
        } else {
          pending.resolve(message);
        }
      }
    }
  }

  rejectAll(error) {
    for (const [id, pending] of this.pending) {
      clearTimeout(pending.timer);
      pending.reject(error);
      this.pending.delete(id);
    }
  }

  dispose() {
    if (!this.process) {
      return;
    }
    try {
      if (this.process.stdin.writable) {
        const id = this.nextId++;
        this.process.stdin.write(`${JSON.stringify({ id, type: 'shutdown' })}\n`);
      }
    } catch (_error) {
      // The process may already be gone.
    }
    setTimeout(() => {
      if (this.process && !this.process.killed) {
        this.process.kill();
      }
    }, 500);
  }
}

class MarvisCodeActionProvider {
  provideCodeActions(document, _range, context) {
    if (!context.diagnostics.length) {
      return [];
    }
    return context.diagnostics.flatMap((diagnostic) => {
      const explain = new vscode.CodeAction('Ask Marvis to explain this diagnostic', vscode.CodeActionKind.QuickFix);
      explain.command = {
        command: 'marvis.explainDiagnostic',
        title: 'Ask Marvis to explain this diagnostic',
        arguments: [document.uri, diagnostic]
      };
      const fix = new vscode.CodeAction('Ask Marvis to fix near this diagnostic', vscode.CodeActionKind.QuickFix);
      fix.command = {
        command: 'marvis.fixNearCursor',
        title: 'Ask Marvis to fix near this diagnostic'
      };
      return [explain, fix];
    });
  }
}

function resolveRuntime(context, workspaceRoot) {
  const config = vscode.workspace.getConfiguration('marvis');
  const configured = String(config.get('runtimePath') || '').trim();
  const binaryName = process.platform === 'win32' ? 'lite-code.exe' : 'lite-code';
  if (configured) {
    return {
      command: configured,
      args: ['--vscode-stdio'],
      cwd: workspaceRoot
    };
  }

  const packagedCandidates = [
    context.asAbsolutePath(path.join('bin', `${process.platform}-${process.arch}`, binaryName)),
    context.asAbsolutePath(path.join('bin', binaryName))
  ];
  for (const candidate of packagedCandidates) {
    if (fs.existsSync(candidate)) {
      return {
        command: candidate,
        args: ['--vscode-stdio'],
        cwd: workspaceRoot
      };
    }
  }

  const runtimeRoot = path.resolve(context.extensionPath, '..', '..');
  const debugBinary = path.join(runtimeRoot, 'target', 'debug', binaryName);
  if (context.extensionMode === vscode.ExtensionMode.Development && fs.existsSync(debugBinary)) {
    return {
      command: debugBinary,
      args: ['--vscode-stdio'],
      cwd: runtimeRoot
    };
  }

  throw new Error(
    'Marvis runtime binary was not found. Set marvis.runtimePath to a built lite-code binary or install an extension package that includes bin/lite-code.'
  );
}

function collectOpenEditors() {
  const editors = new Map();
  for (const editor of vscode.window.visibleTextEditors) {
    const ref = editorRef(editor);
    if (ref) {
      editors.set(ref.path, ref);
    }
  }
  for (const group of vscode.window.tabGroups.all) {
    for (const tab of group.tabs) {
      const uri = tab.input && tab.input.uri;
      if (uri && uri.scheme === 'file' && !editors.has(uri.fsPath)) {
        editors.set(uri.fsPath, {
          path: uri.fsPath,
          language_id: undefined,
          is_dirty: tab.isDirty
        });
      }
    }
  }
  return Array.from(editors.values());
}

function collectDiagnostics(workspaceRoot) {
  const diagnostics = [];
  for (const [uri, entries] of vscode.languages.getDiagnostics()) {
    if (uri.scheme !== 'file' || !isPathInside(workspaceRoot, uri.fsPath)) {
      continue;
    }
    for (const diagnostic of entries) {
      diagnostics.push({
        id: `${uri.fsPath}:${diagnostic.range.start.line}:${diagnostic.range.start.character}:${diagnostic.message}`,
        path: uri.fsPath,
        range: textRange(diagnostic.range),
        severity: severity(diagnostic.severity),
        message: truncate(diagnostic.message, 2000),
        source: diagnostic.source,
        code: diagnostic.code ? String(diagnostic.code) : undefined
      });
    }
  }
  return diagnostics.slice(0, 200);
}

function editorRef(editor) {
  if (!editor || editor.document.uri.scheme !== 'file') {
    return undefined;
  }
  return {
    path: editor.document.uri.fsPath,
    language_id: editor.document.languageId,
    is_dirty: editor.document.isDirty
  };
}

function cursorContextFor(editor) {
  if (!editor || editor.document.uri.scheme !== 'file') {
    return undefined;
  }
  const document = editor.document;
  const cursor = editor.selection.active;
  const line = document.lineAt(cursor.line).text;
  const wordRange = document.getWordRangeAtPosition(cursor);
  const symbolHint = wordRange ? document.getText(wordRange) : undefined;
  const start = Math.max(0, cursor.line - 20);
  const end = Math.min(document.lineCount - 1, cursor.line + 20);
  const lines = [];
  for (let i = start; i <= end; i += 1) {
    lines.push(`${i + 1}: ${document.lineAt(i).text}`);
  }
  return {
    path: document.uri.fsPath,
    line: cursor.line,
    character: cursor.character,
    symbol_hint: symbolHint,
    text_before: line.slice(0, cursor.character),
    text_after: line.slice(cursor.character),
    surrounding_text: truncate(lines.join('\n'), 8000)
  };
}

function position(pos) {
  return {
    line: pos.line,
    character: pos.character
  };
}

function textRange(range) {
  return {
    start: position(range.start),
    end: position(range.end)
  };
}

function severity(value) {
  switch (value) {
    case vscode.DiagnosticSeverity.Error:
      return 'error';
    case vscode.DiagnosticSeverity.Warning:
      return 'warning';
    case vscode.DiagnosticSeverity.Information:
      return 'information';
    case vscode.DiagnosticSeverity.Hint:
      return 'hint';
    default:
      return 'unknown';
  }
}

function readOnlyApproval() {
  return {
    allow_workspace_write: false,
    allow_shell: false,
    allow_risky_shell: false,
    allow_git_write: false,
    allow_network: false
  };
}

function patchApproval() {
  return {
    allow_workspace_write: true,
    allow_shell: true,
    allow_risky_shell: false,
    allow_git_write: false,
    allow_network: false
  };
}

function riskyApproval() {
  return {
    allow_workspace_write: true,
    allow_shell: true,
    allow_risky_shell: true,
    allow_git_write: true,
    allow_network: true
  };
}

function approvalLabel(approval) {
  const value = approval || readOnlyApproval();
  if (value.allow_risky_shell || value.allow_git_write || value.allow_network) {
    return 'risky shell';
  }
  if (value.allow_workspace_write) {
    return value.allow_shell ? 'patch and checks' : 'workspace write';
  }
  if (value.allow_shell) {
    return 'read plus safe shell';
  }
  return 'read only';
}

function selectedText(editor) {
  if (!editor || editor.selection.isEmpty) {
    return '';
  }
  return editor.document.getText(editor.selection);
}

function taskKind(task) {
  if (!task.definition) {
    return undefined;
  }
  return task.definition.type || task.source;
}

function rangeLabel(range) {
  if (!range) {
    return 'unknown';
  }
  return `${range.start.line + 1}:${range.start.character + 1}-${range.end.line + 1}:${range.end.character + 1}`;
}

function isPathInside(root, candidate) {
  const relative = path.relative(root, candidate);
  return relative === '' || (!relative.startsWith('..') && !path.isAbsolute(relative));
}

function truncate(text, max) {
  if (!text || text.length <= max) {
    return text || '';
  }
  return `${text.slice(0, max)}\n... truncated`;
}

function truncateTail(text, max) {
  if (!text || text.length <= max) {
    return text || '';
  }
  return `... truncated\n${text.slice(text.length - max)}`;
}

function renderPanelHtml() {
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <style>
    :root {
      color-scheme: light dark;
      font-family: var(--vscode-font-family);
      font-size: var(--vscode-font-size);
    }
    body {
      margin: 0;
      padding: 0;
      background: var(--vscode-editor-background);
      color: var(--vscode-editor-foreground);
    }
    .toolbar {
      display: flex;
      gap: 6px;
      padding: 10px;
      border-bottom: 1px solid var(--vscode-panel-border);
      position: sticky;
      top: 0;
      background: var(--vscode-editor-background);
      z-index: 1;
    }
    button {
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
      border: 0;
      border-radius: 4px;
      padding: 6px 10px;
      cursor: pointer;
    }
    button.secondary {
      background: var(--vscode-button-secondaryBackground);
      color: var(--vscode-button-secondaryForeground);
    }
    main {
      padding: 12px;
      display: grid;
      gap: 12px;
    }
    section {
      border: 1px solid var(--vscode-panel-border);
      border-radius: 6px;
      padding: 10px;
    }
    h2 {
      margin: 0 0 8px;
      font-size: 13px;
      font-weight: 700;
    }
    .summary {
      white-space: pre-wrap;
      color: var(--vscode-descriptionForeground);
    }
    .segment {
      padding: 8px 0;
      border-top: 1px solid var(--vscode-panel-border);
    }
    .segment:first-child {
      border-top: 0;
    }
    .meta {
      color: var(--vscode-descriptionForeground);
      font-size: 12px;
    }
    .log {
      white-space: pre-wrap;
      line-height: 1.45;
    }
    .log-row {
      border-top: 1px solid var(--vscode-panel-border);
      padding: 8px 0;
    }
    .kind {
      color: var(--vscode-descriptionForeground);
      font-size: 11px;
      text-transform: uppercase;
    }
  </style>
</head>
<body>
  <div class="toolbar">
    <button id="ask">Ask</button>
    <button class="secondary" id="refresh">Refresh</button>
    <button class="secondary" id="checkAutonomy">Check</button>
    <button class="secondary" id="runCommand">Run Command</button>
    <button class="secondary" id="recordFailure">Record Failure</button>
  </div>
  <main>
    <section>
      <h2>Status</h2>
      <div id="summary" class="summary">Waiting for Marvis status...</div>
    </section>
    <section>
      <h2>Active Segments</h2>
      <div id="segments"></div>
    </section>
    <section>
      <h2>Suggestion</h2>
      <div id="suggestion" class="summary">No suggestion yet.</div>
    </section>
    <section>
      <h2>Processes</h2>
      <div id="processes"></div>
    </section>
    <section>
      <h2>Trace</h2>
      <div id="log" class="log"></div>
    </section>
  </main>
  <script>
    const vscode = acquireVsCodeApi();
    const summary = document.getElementById('summary');
    const segments = document.getElementById('segments');
    const suggestion = document.getElementById('suggestion');
    const processes = document.getElementById('processes');
    const log = document.getElementById('log');
    document.getElementById('ask').addEventListener('click', () => vscode.postMessage({ command: 'ask' }));
    document.getElementById('refresh').addEventListener('click', () => vscode.postMessage({ command: 'refresh' }));
    document.getElementById('checkAutonomy').addEventListener('click', () => vscode.postMessage({ command: 'checkAutonomy' }));
    document.getElementById('runCommand').addEventListener('click', () => vscode.postMessage({ command: 'runCommand' }));
    document.getElementById('recordFailure').addEventListener('click', () => vscode.postMessage({ command: 'recordFailure' }));
    window.addEventListener('message', (event) => render(event.data));
    function escapeHtml(value) {
      return String(value || '').replace(/[&<>"']/g, (ch) => ({
        '&': '&amp;',
        '<': '&lt;',
        '>': '&gt;',
        '"': '&quot;',
        "'": '&#39;'
      }[ch]));
    }
    function render(state) {
      const report = state.report || {};
      summary.textContent = report.summary || 'No status report yet.';
      segments.innerHTML = (report.active_segments || []).map((segment) => {
        const files = (segment.files || []).slice(0, 4).join(', ');
        return '<div class="segment"><div><strong>' + escapeHtml(segment.kind) + '</strong></div>' +
          '<div>' + escapeHtml(segment.summary) + '</div>' +
          '<div class="meta">risk=' + escapeHtml(segment.risk_level) +
          ' files=' + escapeHtml(files || 'none') + '</div></div>';
      }).join('') || '<div class="meta">No active segments.</div>';
      if (state.suggestion) {
        const route = state.suggestion.route || {};
        const task = route.task || {};
        const agent = route.agent || {};
        const evidence = (task.evidence || []).slice(0, 4).join('; ');
        suggestion.innerHTML = '<div><strong>' + escapeHtml(task.title || 'Suggested task') + '</strong></div>' +
          '<div>' + escapeHtml(task.prompt || '') + '</div>' +
          '<div class="meta">agent=' + escapeHtml(agent.label || agent.id || 'unknown') +
          ' risk=' + escapeHtml(task.risk_level) +
          ' approval=' + escapeHtml(approvalLabel(state.suggestion.required_approval)) + '</div>' +
          '<div class="meta">' + escapeHtml(evidence || 'No evidence listed.') + '</div>' +
          '<div style="display:flex;gap:6px;margin-top:8px">' +
          '<button id="acceptSuggestion">Accept</button>' +
          '<button class="secondary" id="dismissSuggestion">Dismiss</button></div>';
        document.getElementById('acceptSuggestion').addEventListener('click', () =>
          vscode.postMessage({ command: 'acceptSuggestion', suggestionId: state.suggestion.suggestion_id })
        );
        document.getElementById('dismissSuggestion').addEventListener('click', () =>
          vscode.postMessage({ command: 'dismissSuggestion', suggestionId: state.suggestion.suggestion_id })
        );
      } else if (state.autonomyDecision && state.autonomyDecision.type === 'idle') {
        suggestion.textContent = 'Idle: ' + (state.autonomyDecision.reason || 'no action');
      } else if (state.autonomyDecision && state.autonomyDecision.type === 'suppressed') {
        suggestion.textContent = 'Suppressed: ' + (state.autonomyDecision.reason || 'dismissed');
      } else {
        suggestion.textContent = report.suggestion ? report.suggestion.message : 'No suggestion yet.';
      }
      processes.innerHTML = (state.processes || []).map((process) => {
        return '<div class="segment"><div><strong>' + escapeHtml(process.process_id) + '</strong> ' +
          escapeHtml(process.state) + '</div><div>' + escapeHtml(process.prompt_preview) + '</div>' +
          '<div class="meta">tools=' + escapeHtml(process.tool_calls_used) + '/' + escapeHtml(process.max_tool_calls) +
          ' write=' + escapeHtml(process.allow_workspace_write) + ' risky=' + escapeHtml(process.allow_risky_shell) +
          '</div></div>';
      }).join('') || '<div class="meta">No process events yet.</div>';
      log.innerHTML = (state.log || []).map((row) => {
        return '<div class="log-row"><div class="kind">' + escapeHtml(row.kind) + ' ' + escapeHtml(row.at) +
          '</div><div>' + escapeHtml(row.text) + '</div></div>';
      }).join('') || '<div class="meta">No trace events yet.</div>';
    }
  </script>
</body>
</html>`;
}

module.exports = {
  activate,
  deactivate
};
