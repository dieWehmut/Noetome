# B03-T03 Schema3 组件审查

本组件冻结于B03-T02基线`058a8b353dd41cff7840a2995dedde032a41db9a`之上。独立子代理只读审查四个文件及实际测试日志，没有修改源码或运行后续产品阶段，结论为无可行动的Important/P1/P2问题。本记录只覆盖Schema及项目只读路由，PDF联合模型、真实CLI和Writer仍须单独验收。

| 冻结文件 | 实际SHA256 |
| --- | --- |
| source_schema.rs | c0edd0fa39891681331256a8eca8804beb4690dc5e9ffb7394c33532dbab8171 |
| source_schema3.sql | d688730dbe7e649867a0c11d493796b18588f7c2e66d8db038b7b8b984f8c780 |
| source_schema3_tests.rs | f6a165b74a43cd4350378c5108e75b50d0cb67a88a1ca63b23c46dcc800c4e6f |
| project.rs | 8d5382097ad9aafe23bbbc3faccaef68bf695df0f9e7fe30b54dbc08edfa15c5 |

冻结原字节存于[审查包](assets/B03-T03-schema-reviewed.zip)，避免Windows工作树换行转换影响历史指纹。Schema1/2 SQL与基线相同；Schema3恰有十表和契约规定的字段，旧sources/source_spans/source_imports的DDL保留，PDFVersion独立acquired分支，复合外键保证同parse及同version引用。

实际定向运行`cargo_local.ps1 test -p noetome-cli --lib source_schema::tests --locked --offline`为[20项通过](assets/B03-T03-schema-green.log)，`project::tests`为[13项通过](assets/B03-T03-project-green.log)，均exit0、0 ignored。日志保留当时其他进行中模块的unused警告；不能将这些组件测试替代本项最终全仓回归。

两种升级入口取得BEGIN IMMEDIATE后再次核对原候选，保留旧五表所有SQLite值与rowid；Schema1先建立原Schema2，Schema2按子到父删除、父到子恢复，唯一有意改变的原列值是metadata schema_version。未知对象或不一致元数据拒绝，Schema3不重建、不降级。真实第二读连接阻止COMMIT时，旧记录、数据库字节、user_version均还原，没有journal残留；两个busy-handler/Condvar锁交接RED证明外部在等待锁期间提交新project_id时原实现会错误覆盖，现在均拒绝并保留竞争者字节。

独立复核者使用本地Python/SQLite3.45.3，只在内存中执行冻结迁移SQL：1→3保留一行metadata；2→3保留1 metadata、2 Source、3 Version、3 Span、6 Import的全部值和rowid，PDF五表为空。它没有重跑Rust。主执行者随后用所选工作区Python/SQLite3.53.1[重放相同审查逻辑](assets/B03-T03-schema-audit.py)，[实际输出](assets/B03-T03-schema-audit.json)exit0；这是明确标注的重放，不冒称另一位审阅者。

初始化仍为Schema1；project inspect继续只读，检查已知版本、配置/metadata/PRAGMA及完整DDL配对，不自行迁移或修复。以上未验证PDF来源图、解析缓存、故障发布、任意断电、跨平台或性能目标；相关证据在本项其余验收中提供。
