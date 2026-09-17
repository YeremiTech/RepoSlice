const vscode = require("vscode");
const cp = require("child_process");

function settings() {
  const config = vscode.workspace.getConfiguration("reposlice");
  return {
    executable: config.get("executablePath", "reposlice"),
    workspaceId: config.get("workspaceId", "default")
  };
}

function run(args, cwd) {
  const { executable } = settings();
  return new Promise((resolve, reject) => {
    cp.execFile(executable, args, { cwd, windowsHide: true, maxBuffer: 16 * 1024 * 1024 }, (error, stdout, stderr) => {
      if (error) return reject(new Error((stderr || error.message).trim()));
      resolve(stdout.trim());
    });
  });
}

function rootFolder() {
  const editorUri = vscode.window.activeTextEditor?.document.uri;
  const folder = editorUri ? vscode.workspace.getWorkspaceFolder(editorUri) : vscode.workspace.workspaceFolders?.[0];
  return folder?.uri.fsPath;
}

async function showJson(title, content) {
  const document = await vscode.workspace.openTextDocument({ language: "json", content });
  await vscode.window.showTextDocument(document, { preview: true });
  vscode.window.setStatusBarMessage(title, 4000);
}

function activate(context) {
  context.subscriptions.push(vscode.commands.registerCommand("reposlice.analyzeWorkspace", async () => {
    const root = rootFolder();
    if (!root) return vscode.window.showWarningMessage("Open a folder before running RepoSlice.");
    const { workspaceId } = settings();
    try {
      await run(["add-repository", workspaceId, root, "--format", "json"], root);
      const output = await run(["scan-workspace", workspaceId, "--format", "json"], root);
      await showJson("RepoSlice analysis complete", output);
    } catch (error) {
      vscode.window.showErrorMessage(`RepoSlice: ${error.message}`);
    }
  }));
}

function deactivate() {}
module.exports = { activate, deactivate };
