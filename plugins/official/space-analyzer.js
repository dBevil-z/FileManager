function main(context,api) {
  const selected=context.selection.find(f=>f.is_dir||f.target_is_dir);
  const result=api.directoryStats(selected?selected.path:context.directory);
  const size=b=> b>=1073741824?(b/1073741824).toFixed(2)+' GiB':b>=1048576?(b/1048576).toFixed(1)+' MiB':b>=1024?(b/1024).toFixed(1)+' KiB':b+' B';
  return {title:'目录空间分析',summary:`${result.files} 个文件 · ${size(result.bytes)}${result.truncated?' · 达到 10 万文件上限':''}${result.skipped?' · 跳过 '+result.skipped+' 个不可读项目':''}`,columns:['名称','大小','路径'],rows:result.largest.map(f=>[f.name,size(f.size),f.path]),reports:[{name:'目录空间报告.json',content:JSON.stringify(result,null,2)}]};
}
