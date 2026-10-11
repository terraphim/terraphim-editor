/* Run inside an actual isolated VS Code extension-test host. No mocked vscode. */
const vscode = require('vscode');
const fs = require('node:fs/promises');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const assert = require('node:assert/strict');
exports.run = async () => {
  const assetDir = process.env.TERRAPHIM_HOST_WASM;
  const reportPath = process.env.TERRAPHIM_SYNONYM_NATIVE_REPORT;
  assert.ok(assetDir && reportPath, 'explicit actual WASM and report paths required');
  const report = {scope:'Actual VS Code TextDocument/native undo-redo with the shared WASM engine; no webview visual/all41 claim', checks:[], outputs:[]};
  let engine;
  try {
    const api = await import(pathToFileURL(path.join(assetDir,'terraphim_editor.mjs')));
    await api.default({module_or_path:await fs.readFile(path.join(assetDir,'terraphim_editor_bg.wasm'))});
    const uri = vscode.Uri.file('/private/tmp/terraphim-synonyms-native-output-20261011.md');
    const original = 'Decision matters. Café 😀\r\n';
    await vscode.workspace.fs.writeFile(uri,Buffer.from(original));
    let doc = await vscode.workspace.openTextDocument(uri);
    await vscode.window.showTextDocument(doc);
    engine = new api.TerraphimHostSession(uri.toString(),'native-synonyms',doc.version,doc.getText());
    const current = JSON.parse(engine.snapshot());
    const proposal = JSON.parse(engine.propose(JSON.stringify({...current,id:1,command:'alt_create',args:{kind:'word',start:0,end:8,text:'Choice'}})));
    const span = proposal.projection.annotations.spans[0].id;
    assert.deepEqual(JSON.parse(engine.synonyms_for(span,'Choice')),[]);
    const edit = new vscode.WorkspaceEdit();
    edit.replace(uri,new vscode.Range(doc.positionAt(0),doc.positionAt(doc.getText().length)),proposal.source);
    assert.equal(await vscode.workspace.applyEdit(edit),true);
    assert.equal(doc.getText(),proposal.source);
    engine.acknowledge(1,doc.version,doc.getText());
    assert.deepEqual(JSON.parse(engine.synonyms_for(span,'Choice')),['decision','choice']);
    report.checks.push('actual native edit acknowledged; Choice and Decision resolve to one KG concept');
    await doc.save();
    assert.equal((await fs.readFile(uri.fsPath)).toString(),proposal.source);
    report.outputs.push({step:'saved',source:doc.getText(),synonyms:JSON.parse(engine.synonyms_for(span,'Choice'))});
    report.checks.push('actual saved Markdown bytes match acknowledged annotated output');
    await vscode.commands.executeCommand('undo');
    assert.equal(doc.getText(),original);
    engine.synchronize(doc.version,doc.getText());
    assert.deepEqual(JSON.parse(engine.synonyms_for(span,'Choice')),[]);
    report.outputs.push({step:'native undo',source:doc.getText(),synonyms:JSON.parse(engine.synonyms_for(span,'Choice'))});
    report.checks.push('native undo removes authored KG synonym membership');
    await vscode.commands.executeCommand('redo');
    assert.equal(doc.getText(),proposal.source);
    engine.synchronize(doc.version,doc.getText());
    assert.deepEqual(JSON.parse(engine.synonyms_for(span,'Choice')),['decision','choice']);
    report.outputs.push({step:'native redo',source:doc.getText(),synonyms:JSON.parse(engine.synonyms_for(span,'Choice'))});
    report.checks.push('native redo restores exact annotated source and KG membership');
    await doc.save();
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    doc = await vscode.workspace.openTextDocument(uri);
    await vscode.window.showTextDocument(doc);
    const diskSource = Buffer.from(await vscode.workspace.fs.readFile(uri)).toString();
    assert.equal(diskSource,proposal.source);
    const reopened = new api.TerraphimHostSession(uri.toString(),'native-reopened',doc.version,diskSource);
    try {
      assert.equal(doc.getText(),proposal.source);
      assert.deepEqual(JSON.parse(reopened.synonyms_for(span,'Choice')),['decision','choice']);
      report.outputs.push({step:'reopen',source:doc.getText(),groups:JSON.parse(reopened.snapshot()).projection.annotations.synonymGroups});
    } finally {reopened.free();}
    report.checks.push('native close/reopen and fresh engine restore typed KG synonyms from saved Markdown');
    report.result='PASS';
  } catch(error) {report.result='FAIL';report.error=String(error.stack || error);throw error;}
  finally {if(engine)engine.free();await fs.writeFile(reportPath,JSON.stringify(report,null,2));}
};
