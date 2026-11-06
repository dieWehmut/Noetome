# B07-T01 Windows focused35 清理失败及真实修复回归

历史01实际35测试=30 ok+4 ERROR+1 skip、unit PID38056/outer54324、exit1，原stderr四项均为NTFS WinError32临时数据库清理失败，每案两次清理 traceback，所以是4个测试ERROR、8个 WinError32 traceback。程序未关闭RO/mutator连接会让临时目录删除失败。原source 21250294/b0f0f1c6与原stderr完整保留，不把先前错误当通过。

历史02实际35测试=34 ok+1 skip、unit PID61272/outer10488、exit0；87cb1580检查器、18d67c31测试两份源原件 before/after逐字相等。冻结 test 使用实际 SQLite writer spill/cold与ownPID kill、正常Windows路径和规范Owner/只读status、普通三CLI start/status/resume六单元/唯一发布桥接检查。原runner.scope误写two main probes未改，计数以冻结测试真实assert3CLI及实际ok执行为依据；不能据旧字符串计两次。

两次各自完整top-level launcher/runner/raw/result/source before/after及31 source-copy成员收录，包括17作者包、8旧fixture、检查器/测试/legacy helper和原生成pyc。作者冻结16 member字节独立重验。成功测试的绿色02临时目录已由测试清理，因此本包没有逐CLI独立stdout/stderr或该临时Run DB；证据是原unit stdout/stderr、完整源及其执行断言。01失败残留的C:临时目录不扫描、不纳入本包。进程所有者review_scan_tools明确确认两轮outer/test/writer/CLI已结束，所有原root稳定；该确认不是重新生成的OS进程原raw。

固定Native04 PE ee5dde73真实SHA/长度与构建回执绑定。它的完整本体已在native-history final bc2e ZIP，此小包明确不重复PE、driver或GB缓存；不把历史04 PE的专项结果推为Native06最终验收或Core05全部检查。

归档工具第一次错误地把4个测试ERROR等同4个WinError32 traceback，原计数断言失败；新归档按4个ERROR测试与8个双重清理traceback分别核对。该包装器失败脚本与物理stdout/stderr一并保留，不是产品RED。

逐member CRC/SHA/长度与原件的WSL读取句柄device/inode、mtime和byte SHA前后全部核对；此读取身份证据不是新Win32 file-ID采集。实际模型/付费调用0，无生产Vault/凭据读取。所有公开写入仍由root在最终source gates之后进行。
