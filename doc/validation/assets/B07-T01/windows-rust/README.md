# Native06 完整Rust与Windows程序体

[actual.json](actual.json)和[原始包](originals.zip)保存实际Windows fmt/check/build/test、23 suite/756 passed/0 failed/0 ignored，以及完整PE/driver原件、verbose构建、锁定注册表源与2002源码前后守卫。

该轮Python采用之前35项验证器，并且仅发现Windows PowerShell引擎：437=429通过/8跳过是历史作用域，不能用作最终完整Python验收。最终36项验证器、原441个测试身份及两个PowerShell引擎的477项实际回归见[Native08](../windows-python/actual.json)及[原始包](../windows-python/originals.zip)。Native08并未重新执行Cargo；四份89/CLI125/workspace125/1864逐文件清单相等证明这里的Rust执行证据适用于最终产品和受影响旧Rust测试。两个125清单成员不同。
