# 离线抽取网关作者夹具

source.txt 与 expected.json 由作者在 fake 网关实现前声明。文字是工程测试数据，速度与温度不代表外部科学事实。response.json 是这份 gold 的单一明确 fake 回放；不运行模型、不修改 gold、不证明科学正确率或全来源覆盖。

真实 UUID、文本 Span、PDF parse_id/区域/资源 SHA 由测试实际导入后取得；作者 quote/数值/单位/条件不从网关输出推导。PDF 正例使用 ../pdf-text-v1/single-page.pdf 的已声明正文及真实已登记页面资源，负例包括 failed/scanned 页面。

普通 CLI 验证器为 scripts/validate_extraction_gateway.py；它创建全新测试根，自行检查输入来源 SQL/文件 SHA、mtime/目录清单、候选内容与 gold，并真实触发超时及运行时取消。fake token 使用 UTF-8 字节计量，不等于真实模型 tokenizer。
