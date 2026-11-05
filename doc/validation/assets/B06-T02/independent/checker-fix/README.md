交叉审查的原始假通过记录保留在 blocker-red/ 与 path-red/。before-*.py 保存修复前原字节；没有覆盖原来的成功退出记录。

修复后从实际来源推导阻断诊断对象和固定原因，并仅在本 process checker 增加规范版本资源路径、项目身份、raw→normalized 与 blocks-v1 完整选定版本范围重建。旧共享 actual_source 未修改，Rust 与 16 份冻结作者产物未修改。

focused-originals.zip 保存 16 条 focused 命令的原始输出、前后保护指纹、源码和新构造输入；新增两种同步伪造（原文与规范缓存不对应、SQL 截掉否定尾句）均得到旧 checker 0 → 修复后 checker 1 → Rust 拒绝且无输出。原交叉审查的两份伪造输入也原样重放，新 checker 拒绝，正样例仍通过。真实前置标题与空白后的 ordinal=2 正链通过了 4 条普通 CLI 及独立 check。28 项针对性 Python 测试通过。

所有 ZIP 成员完成 CRC、SHA-256 与原件逐字读回。可执行文件仅保留本地路径和指纹。没有重跑第二轮完整 audit；根代理将统一跑最终原生候选验收。fix-review.json 是修复者说明，最终独立复核由另一位审查者记录。
