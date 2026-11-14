function main(context, api) {
  const files = context.selection.filter(f => !f.is_dir && !f.target_is_dir);
  if (!files.length) throw new Error('请在文件浏览器中选中文件，再运行此插件。');
  const expected = context.inputs.expected.trim().toLowerCase();
  const rows = files.map(file => { const result = api.hash(file.path, context.inputs.algorithm); return [file.name, result.hash, expected ? (result.hash === expected ? '一致' : '不一致') : '已计算']; });
  return { title: '文件校验', summary: `已校验 ${rows.length} 个文件 · ${context.inputs.algorithm}`, columns: ['文件','校验值','结果'], rows, reports: [{name:context.inputs.algorithm==='SHA-512'?'SHA512SUMS.txt':'SHA256SUMS.txt', content:rows.map(r=>`${r[1]}  ${r[0].replace(/[\r\n]/g,'_')}`).join('\n')+'\n'}] };
}
