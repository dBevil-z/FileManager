function main(context, api) {
  return {title:context.inputs.greeting,summary:'仅在运行时创建 JavaScript 引擎',columns:['名称','类型'],rows:context.selection.map(file=>[file.name,file.kind])};
}
