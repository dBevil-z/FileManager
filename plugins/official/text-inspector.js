function main(context,api) {
  const files=context.selection.filter(f=>!f.is_dir&&!f.target_is_dir);
  if(!files.length) throw new Error('请选择文本文件（每个文件不超过 1 MiB）。');
  const rows=files.map(f=>{ try {const text=api.readText(f.path);const lines=text.split(/\r\n|\r|\n/);const crlf=(text.match(/\r\n/g)||[]).length;const lf=(text.match(/\n/g)||[]).length-crlf;return [f.name,text.length, text.length?lines.length-(lines.at(-1)===''?1:0):0,crlf&&lf?'混合':crlf?'CRLF':'LF',text.charCodeAt(0)===0xfeff?'有':'无',lines.filter(l=>/[\t ]+$/.test(l)).length,lines.filter(l=>l.length>200).length];}catch(e){return[f.name,'读取失败',e.message,'','','',''];}});
  return {title:'文本健康检查',summary:`${rows.length} 个文件 · 原文件保持不变`,columns:['文件','字符','行数','换行','BOM','行尾空格','超长行'],rows,reports:[{name:'文本检查.json',content:JSON.stringify(rows,null,2)}]};
}
