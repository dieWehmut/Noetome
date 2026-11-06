# Candidate05 helper 独立源码差异审查

仅一行52B变更，在 `#[cfg(test)]` 的 fixture setup：创建临时base后将base真实canonicalize，再派生fixture和Run路径。逐字证明当前05减去该唯一整行就精确等于捕获04全文，首个且唯一cfg(test)之前全部生产字节相同。Owner/影像/metadata payload/原输入/输出、journal结构和RecoveryWitness生产门禁没有修改。此变更让测试入口采用与正常路径选择相同的真实路径表示，没有宽松接受另一Owner或外来journal。

独立重读build05源ZIP125全部成员CRC/SHA/长度并比较当前原字节，全部相同，source before/after相等。读取helper实际stdout的12个唯一测试名称，全部ok；退出0、stderr0B，source before/after相等。正常、foreign、shared-only、cold、损坏、身份/链接、输入变更与witness相关测试均在该真实12项内，详见JSON和原raw。没有执行新的Cargo或CLI，focused helper不等于完整Linux Rust；Windows收据由对应原生分工核对。

程序05只标干净构建身份通过、完整Python/矩阵/native门禁待验。旧04与真实native04两个失败、52B修复和旧程序归档全部保留。
