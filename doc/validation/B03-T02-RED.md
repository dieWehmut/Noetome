# B03-T02 实际红灯记录

在任何 PDF adapter/CLI 实现之前，已运行：

```powershell
./scripts/cargo_local.ps1 test -p noetome-cli --test pdf_parse --locked --offline
```

结果：退出 1；成功编译后 5 组全部失败、0 ignored。单/多页与位置、边界矩阵、原文件移动后 inspect、新根/缺失输入保护及语义篡改均因普通入口还没有 `parse-pdf` / `inspect-pdf` 而失败。首个正常用例 stderr 为 `CLI_INVALID_ARGUMENT：参数无效或缺少项目路径。`，边界期望退出 3 实际退出 2；不是测试语法或依赖编译错误。

固定输入与 `expected.json` 由独立 authoring generator 先创建，未运行待测 PDF 解析器。后续 Core 定位负例和修复红/绿另在最终验收中追加，不能把这次缺入口失败视为所有定位校验已经证明有效。
