# B03-T02 独立真实 CLI 支持矩阵

固定预期来自作者 PDF 绘制操作与事先 Poppler 视觉记录；不由待测 parser 输出生成。每项实际 parse 与只读 inspect，非 parsed 的退出码为3。

| 自造样例 | 状态 | 确认总页数 | parsed / review / failed | parse 实际ms | inspect 实际ms | 资源字节 |
| --- | --- | --- | --- | --- | --- | --- |
| single-page.pdf | parsed | 1 | 1 / 0 / 0 | 213.015 | 19.564 | 25955 |
| multiple-pages.pdf | parsed | 3 | 3 / 0 / 0 | 207.386 | 24.161 | 37360 |
| crop-offset.pdf | parsed | 1 | 1 / 0 / 0 | 198.838 | 18.467 | 17283 |
| rotations.pdf | needs_review | 8 | 1 / 7 / 0 | 231.789 | 36.539 | 99807 |
| scanned-page.pdf | needs_review | 1 | 0 / 1 / 0 | 192.838 | 17.239 | 3946 |
| blank-page.pdf | needs_review | 1 | 0 / 1 / 0 | 191.929 | 16.983 | 2218 |
| damaged.pdf | failed | unknown | 0 / 0 / 0 | 189.19 | 15.668 | 920 |
| encrypted.pdf | failed | unknown | 0 / 0 / 0 | 195.259 | 16.303 | 2795 |
| table-page.pdf | needs_review | 1 | 0 / 1 / 0 | 204.167 | 18.289 | 6618 |
| formula-page.pdf | needs_review | 1 | 0 / 1 / 0 | 193.962 | 19.118 | 5059 |
| image-page.pdf | needs_review | 1 | 0 / 1 / 0 | 194.192 | 17.646 | 9491 |
| columns-page.pdf | needs_review | 1 | 0 / 1 / 0 | 194.994 | 18.168 | 13983 |
| partial-page-failure.pdf | failed | 2 | 1 / 0 / 1 | 193.122 | 20.579 | 10901 |
| layout-order.pdf | parsed | 1 | 1 / 0 / 0 | 191.571 | 17.863 | 8326 |

时间含原生 CLI 启动、Python、Core及最终报告写入；一次小样例实测，无缓存或统计吞吐承诺。资源字节为raw/text/manifest总和，完整文件SHA在JSON；不等于进程内存。

未运行：memory/CPU sampling、1000-page throughput、24-hour stability、cross-platform compatibility、human scientific gold standard、OCR、model/knowledge recall or scientific accuracy、physical UserUnit scaling by independent renderer、pdftotext unavailable。
