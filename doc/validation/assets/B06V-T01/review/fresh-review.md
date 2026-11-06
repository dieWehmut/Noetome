# B06V-T01 fresh只读验收审查

执行者：独立fresh review子代理。范围只限本任务；未编辑产品、校验器、测试、标准或账本，未执行commit/push，也未启动B06V-T02/03。

结论：本次fresh review发现一个有实际普通CLI反例的完成阻碍，状态pending。清晰原图的黑色图例/元素背景上白字可辨，但有限SVG将背景和全部关键文字都编译为黑色，并宣称svg_redraw成功。父代理已收到证据，后续修复和GREEN由父代理/对应执行者记录；本文件保留本次RED，不将之后结果倒填成这次审查通过。

## 实际反例

- 来源是本次新自造400×250整张PNG：黑色Payload矩形内白色Payload/mg；黑色图例内白色Dark/Selected payload。结构声明是作者手工输入，color=#000000，complete=true，readability=clear，明确选择三个普通文件。
- 运行前向父代理报告black fill与固定black text会使关键语义消失，再创建上述新原件。未修改12作者fixture或独立expected。
- 普通CLI实际退出0，decision={representation:svg_redraw,fallback_reasons:[],issue_codes:[]}。SVG两个背景为#000000，四个关键text也为#000000，实际标签/单位/图例意义不可见。
- 本例违反关键语义可见与不能安全保真布局应回退完整原图的任务要求。这个问题属于有限手工声明重绘范围，不要求实现OCR、自动图像理解或数据推断。建议已知但不安全颜色保守回退原图，独立分类及同步fake redraw拒绝须跟随验证。
- receipt、stdout/stderr、source三原件、profile及实际新Vault见contrast-probe/；source三原件与输出副本逐字节相同，核对已实际执行。源PNG SHA为7ad9fbb707479a45fe75ff62a0c323588f6c3ad445aff6e3ce1c466ef5ecfcad。

## 执行及环境

真实命令（通过subprocess.run执行、capture stdout/stderr）：

```
/home/lyinn/project/Noetome/doc/validation/assets/B06V-T01/review/contrast-probe/noetome knowledge image /home/lyinn/project/Noetome/doc/validation/assets/B06V-T01/review/contrast-probe/source /home/lyinn/project/Noetome/doc/validation/assets/B06V-T01/review/contrast-probe/profile.json /home/lyinn/project/Noetome/doc/validation/assets/B06V-T01/review/contrast-probe/new-Vault
```

CLI来自target/debug/noetome逐字节复制，复制文件link count=1后调用，没有Cargo rebuild。CLI SHA097ba5a523d81320afeb8310a90f70b00660e5caf9410f9f2d0c2b63b467f885。源码当时指纹见contrast-probe/pins-before.json。本次新PNG使用已有/tmp/noetome-b05-t03-python/bin/python与Pillow生成，不安装依赖。

第一次准备错误也如实记录：默认python缺PIL，ModuleNotFoundError出现在创建目录之前；紧随的CLI命令因不存在复制可执行文件退出127。这两次未创建产品/输出，不是产品RED。改用父代理已提供的绝对Python路径后才生成原件并完成上述真实产品RED。

## 审查覆盖和限制

已读根AGENTS、STATUS、TODO、tasks本任务、NEXT_STEPS、doc入口、IMAGE_FIDELITY_CONTRACT与TESTING相关标准。已审image_model、image_input、image_fidelity、image_render、image_guard_tests、CLI tests/image_fidelity、scripts/verify_image及tests/test_verify_image。

源码层面：viewBox/画布保留整图尺寸；forward/reverse箭头polygon分别绑定端点；direction/effect/unit/legend_id独立字段保存；实际shape与空间几何、SVG元素顺序和完整属性、Markdown章节/字段关联/字面围栏、双报告壳及冻结作者指纹都有明确检查。报告诚实限定pending/not_run/unpublished/零收费，不把手工结构声明称为自动识别。输入库存与原件pin、PNG完整decode/CRC/EOF/限额、祖先持有期重验、限定逐文件cleanup都有明确实现及针对测试。除颜色反例外，本次没有取得其他可复现重要缺陷。

这些源码观察不代替实际守卫/同步篡改/完整Rust或Python/Obsidian验收：本次只实际跑了一个独立新反例、读回原件和新Vault及指纹，未运行完整回归、平台保护、Obsidian或浏览器渲染，也没有推定它们通过。本任务完成还需颜色修复GREEN及父代理正在进行的完整独立审计/原生最终源码验收。
