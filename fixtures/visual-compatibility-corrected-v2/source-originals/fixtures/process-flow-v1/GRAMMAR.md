# process_source_zh_v1 作者语法

输入是一段显式选定的 UTF-8 文字 RequestUnit，LF 分行；最后允许一个 LF。不接收 Mermaid、坐标/布局、候选图或任意自然语言关系推断。每个非空声明行必须完整匹配一个模板，未知尾句、否定词或未消费字符使 source_review=needs_review、无成功 process IR。

词法：`<id>` 使用显式小写字母开头、后接小写字母/数字/下划线，最多32个ASCII字节（本作者使用s1/d1/r1）。`<string>` 和 `<strings>` 是完整 JSON string 与 JSON string array 词面；允许 JSON 自身的合法转义、非ASCII文字和JSON数组内空白。标签中的 `。`、双引号或竖线不作为模板分隔符；用JSON decoder确定词面终点。全部数组必填，可为[]，表示“选定来源未列出”，不是不存在或已知没有。JSON字符串内的\n/\t解码为LF/TAB且保留原quote。决定判据 `<criterion>` 为JSON string或null；null仅表示未列明，不能猜默认判据。

模板中的中文、空格、冒号和句号逐字匹配。文件仅一条过程行，必须首先出现；之后是步骤/判定声明，再是关系声明。本批节点/关系声明顺序不暗示执行先后。每行结束后除行末LF不得有未消费字符。

```text
过程：<string>。入口：<id>。目标：<strings>。条件：<strings>。例外：<strings>。
步骤 <id>：<string>。说明：<string>。输入：<strings>。输出：<strings>。条件：<strings>。例外：<strings>。
判定 <id>：<string>。判据：<criterion>。说明：<string>。输入：<strings>。输出：<strings>。条件：<strings>。例外：<strings>。
关系 <id>：<id> 完成后执行 <id>。条件：<strings>。例外：<strings>。
关系 <id>：<id> 的判据为真时执行 <id>。条件：<strings>。例外：<strings>。
关系 <id>：<id> 的判据为假时执行 <id>。条件：<strings>。例外：<strings>。
关系 <id>：<id> 导致 <id>。条件：<strings>。例外：<strings>。
关系 <id>：<id> 促进 <id>。条件：<strings>。例外：<strings>。
关系 <id>：<id> 抑制 <id>。条件：<strings>。例外：<strings>。
关系 <id>：<id> 与 <id> 相关。条件：<strings>。例外：<strings>。
关系 <id>：<id> 与 <id> 的方向未说明。条件：<strings>。例外：<strings>。
```

关系动词分别产生 sequence、branch(true/false)、causes、promotes、inhibits、correlates、unknown；非branch的branch_outcome必须null。causes为“导致”，不自动增加正号；只有promotes/inhibits保留对应正/负作用。branch引用已声明decision的同一个criterion，其conditions为额外前提，不用于推断判据。correlates/unknown的两个端点仅为来源词面端点，不获得方向。

Gold schema：{schema_version:1,producer:"process_source_gold",author,scientific_quality:"not_run",process,steps,relations,evidence}。process字段title,entry,goals,conditions,exceptions；step字段id,kind,label,description,criterion,inputs,outputs,conditions,exceptions；relation字段id,left,right,kind,branch_outcome,conditions,exceptions。所有字段必填。证据object_id为process或各记录id，quote为完整声明行（不含LF）；start_byte/end_byte是所选RequestUnit内部UTF8半开范围。

source_review只声称所选原文与完整语义gold相符。相关、循环、方向未知、缺分支、未知判据、缺关键连接均可精确核对并保真保存process IR；flow另行阻断。无法匹配的尾句/否定语句必须留原文并阻断review成功，不能忽略它再出成功IR。

Expected是独立表达gold，不作CLI输入；保存profile、review_status、flow_status、representation、issues、完整process/node/edge语义和分母，以及拓扑先后约束。它不包含Mermaid、NotePlan、行列坐标或布局。复杂LF/TAB标签要求ordered_list回退；普通特殊标点仍可flowchart。unverified_flowchart能力profile要求ordered_list并说明“能力未核验”，不能宣称Obsidian不支持。
