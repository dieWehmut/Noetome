//! Author-fixed Markdown literals selected before running the renderer.
//! The table-link variant separately declares the same intentional change in
//! NotePlan and independent expected; no renderer output updates these values.

pub const ALPHA: &str = concat!(
    "# 甲容器\n\n",
    "## 定义与比较\n\n",
    "甲容器是虚构的蓝砂收纳盒。参见乙容器。 [[笔记/乙容器#定义与条件|乙容器]]\n\n",
    "| 比较属性 | 甲容器 | 乙容器 |\n",
    "| --- | --- | --- |\n",
    "| 虚构刻度 | 甲容器的虚构刻度为5 cm。仅用于室温演示；装满后停止读数；破损时不适用。 | 乙容器的虚构刻度为8 cm。仅用于室温演示；空盒时不读数；浸水后不适用。 |\n",
    "| 单价 | 未给出 | 未给出 |\n\n",
);

pub const BETA: &str = concat!(
    "# 乙容器\n\n",
    "## 定义与条件\n\n",
    "乙容器是虚构的红砂收纳盒。参见甲容器。 [[笔记/甲容器#定义与比较|甲容器]]\n\n",
    "1. 乙容器的虚构刻度为8 cm。仅用于室温演示；空盒时不读数；浸水后不适用。\n\n",
);

pub const ALPHA_WITH_TABLE_ALIAS: &str = concat!(
    "# 甲容器\n\n",
    "## 定义与比较\n\n",
    "甲容器是虚构的蓝砂收纳盒。参见乙容器。 [[笔记/乙容器#定义与条件|乙容器]]\n\n",
    "| 比较属性 | 甲容器 | 乙容器 |\n",
    "| --- | --- | --- |\n",
    "| 虚构刻度 | 甲容器的虚构刻度为5 cm。仅用于室温演示；装满后停止读数；破损时不适用。 [[笔记/乙容器#定义与条件\\|乙容器入口]] | 乙容器的虚构刻度为8 cm。仅用于室温演示；空盒时不读数；浸水后不适用。 |\n",
    "| 单价 | 未给出 | 未给出 |\n\n",
);

pub const ALPHA_WITH_ESCAPED_TEXT: &str = concat!(
    "# 甲容器\n\n",
    "## 定义与比较\n\n",
    "甲容器是虚构的蓝砂收纳盒。参见乙容器。 普通明文 x \\< 2 和区间 \\[1\\,2\\]。 [[笔记/乙容器#定义与条件|乙容器]]\n\n",
    "| 比较属性 | 甲容器 | 乙容器 |\n",
    "| --- | --- | --- |\n",
    "| 虚构刻度 | 甲容器的虚构刻度为5 cm。仅用于室温演示；装满后停止读数；破损时不适用。 | 乙容器的虚构刻度为8 cm。仅用于室温演示；空盒时不读数；浸水后不适用。 |\n",
    "| 单价 | 未给出 | 未给出 |\n\n",
);

pub const TABLE_ALIAS_LITERAL: &str = "[[笔记/乙容器#定义与条件\\|乙容器入口]]";
pub const ALPHA_PATH: &str = "笔记/甲容器.md";
pub const BETA_PATH: &str = "笔记/乙容器.md";
