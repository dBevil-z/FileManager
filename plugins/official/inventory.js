function main(context, api) {
  const entries = context.selection.length ? context.selection : api.listDirectory(context.directory);
  const rows = entries.map(e=>[e.name,e.is_dir||e.target_is_dir?'文件夹':e.kind,e.size||0,e.modified?new Date(e.modified*1000).toISOString():'',e.path]);
  const columns=['名称','类型','字节','修改时间','路径'];
  const cell=v=>`"${String(v).replace(/"/g,'""')}"`;
  const safe=v=>/^[=+\-@\t\r]/.test(String(v))?"'"+v:v;
  const md=v=>String(v).replace(/\|/g,'\\|').replace(/[\r\n]/g,' ');
  return { title:'文件清单',summary:`${rows.length} 个项目 · 只读取文件元数据`,columns,rows,
    reports:[
      {name:'文件清单.csv',content:'\uFEFF'+[columns,...rows].map(r=>r.map(v=>cell(safe(v))).join(',')).join('\r\n')},
      {name:'文件清单.md',content:[`# 文件清单\n`,`| ${columns.join(' | ')} |`,`| ${columns.map(()=> '---').join(' | ')} |`,...rows.map(r=>`| ${r.map(md).join(' | ')} |`)].join('\n')},
      {name:'文件清单.json',content:JSON.stringify(entries,null,2)}] };
}
