# B03-T02 独立真实 CLI 支持矩阵

固定预期来自作者 PDF 绘制操作与事先 Poppler 视觉记录；不由待测 parser 输出生成。每项实际 parse 与只读 inspect，非 parsed 的退出码为3。

| 自造样例 | 状态 | 确认总页数 | parsed / review / failed | parse 实际ms | inspect 实际ms | 资源字节 |
| --- | --- | --- | --- | --- | --- | --- |
| single-page.pdf | parsed | 1 | 1 / 0 / 0 | 227.727 | 20.756 | 25955 |
| multiple-pages.pdf | parsed | 3 | 3 / 0 / 0 | 214.879 | 23.409 | 37360 |
| crop-offset.pdf | parsed | 1 | 1 / 0 / 0 | 197.228 | 19.884 | 17283 |
| rotations.pdf | needs_review | 8 | 1 / 7 / 0 | 253.874 | 39.797 | 99807 |
| scanned-page.pdf | needs_review | 1 | 0 / 1 / 0 | 222.49 | 17.725 | 3946 |
| blank-page.pdf | needs_review | 2 | 0 / 2 / 0 | 215.218 | 25.791 | 4220 |
| damaged.pdf | failed | unknown | 0 / 0 / 0 | 195.838 | 14.846 | 920 |
| encrypted.pdf | failed | unknown | 0 / 0 / 0 | 215.727 | 15.91 | 2795 |
| table-page.pdf | needs_review | 1 | 0 / 1 / 0 | 206.655 | 19.813 | 6618 |
| formula-page.pdf | needs_review | 1 | 0 / 1 / 0 | 208.058 | 19.282 | 5059 |
| image-page.pdf | needs_review | 1 | 0 / 1 / 0 | 251.761 | 18.767 | 9491 |
| columns-page.pdf | needs_review | 2 | 0 / 2 / 0 | 223.046 | 23.183 | 28947 |
| partial-page-failure.pdf | failed | 2 | 1 / 0 / 1 | 204.219 | 18.334 | 10901 |
| layout-order.pdf | parsed | 1 | 1 / 0 / 0 | 198.817 | 19.551 | 8326 |

时间含原生 CLI 启动、Python、Core及最终报告写入；一次小样例实测，无缓存或统计吞吐承诺。资源字节为raw/text/manifest总和，完整文件SHA在JSON；不等于进程内存。

未运行：memory/CPU sampling、1000-page throughput、24-hour stability、cross-platform compatibility、human scientific gold standard、OCR、model/knowledge recall or scientific accuracy、physical UserUnit scaling by independent renderer、pdftotext unavailable。
