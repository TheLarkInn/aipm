import { workspace, window, ExtensionContext } from 'vscode';
import {
  CloseAction,
  ErrorAction,
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: ExtensionContext): void {
  const config = workspace.getConfiguration('aipm');
  if (!config.get<boolean>('lint.enable', true)) return;

  const aipmPath = process.env['AIPM_PATH'] ?? config.get<string>('path', 'aipm');

  const serverOptions: ServerOptions = {
    command: aipmPath,
    args: ['lsp'],
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      // Workspace manifest — completions and hover for [workspace.lints]
      { scheme: 'file', pattern: '**/aipm.toml' },
      // Skill files — flat layout (.claude/skills/SKILL.md)
      { scheme: 'file', pattern: '**/skills/SKILL.md' },
      // Skill files — nested layout (.claude/skills/default/SKILL.md)
      { scheme: 'file', pattern: '**/skills/*/SKILL.md' },
      // Agent files — any *.md inside an agents/ directory (NOT a fixed name like AGENT.md)
      { scheme: 'file', pattern: '**/agents/*.md' },
      // Hook config — hooks.json inside a hooks/ directory
      { scheme: 'file', pattern: '**/hooks/hooks.json' },
      // Plugin manifests — aipm.toml directly under .ai/<plugin>/
      { scheme: 'file', pattern: '**/.ai/*/aipm.toml' },
      // Plugin JSON manifests — .ai/<plugin>/.claude-plugin/plugin.json
      { scheme: 'file', pattern: '**/.ai/*/.claude-plugin/plugin.json' },
      // Marketplace manifest — .ai/.claude-plugin/marketplace.json
      { scheme: 'file', pattern: '**/.ai/.claude-plugin/marketplace.json' },
    ],
    errorHandler: {
      error: () => ({ action: ErrorAction.Continue }),
      closed: () => {
        void window.showErrorMessage(
          'aipm language server stopped. Check that the `aipm` binary is installed and accessible via PATH (or set `aipm.path`).',
        );
        return { action: CloseAction.DoNotRestart };
      },
    },
  };

  client = new LanguageClient(
    'aipm',
    'aipm Language Server',
    serverOptions,
    clientOptions,
  );

  client.start();
  context.subscriptions.push(client);
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
