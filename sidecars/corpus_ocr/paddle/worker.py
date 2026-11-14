"""Pinned PaddleOCR full layout + VL-1.6 comparison on frozen original page PNGs."""
import argparse,importlib.metadata,json,os,sys,time
from pathlib import Path

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--input',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--runtime',type=Path,required=True);args=parser.parse_args()
    args.output.mkdir(parents=True,exist_ok=False)
    os.environ.update({'PADDLE_PDX_CACHE_HOME':str(args.runtime/'model-cache'),'HF_HUB_DISABLE_IMPLICIT_TOKEN':'1','HF_HUB_DISABLE_TELEMETRY':'1','HF_HOME':str(args.runtime/'huggingface-cache')})
    assert sys.platform=='win32' and sys.version.split()[0]=='3.12.14'
    assert importlib.metadata.version('paddleocr')=='3.7.0'
    import paddle
    from paddleocr import PaddleOCRVL
    assert paddle.__version__=='3.2.1' and paddle.is_compiled_with_cuda()
    started=time.perf_counter()
    pipeline=PaddleOCRVL(pipeline_version='v1.6',device='gpu:0',use_doc_orientation_classify=False,use_doc_unwarping=False,use_layout_detection=True,use_chart_recognition=True,use_ocr_for_image_block=True,markdown_ignore_labels=[],use_queues=False)
    loaded=time.perf_counter()
    for index,result in enumerate(pipeline.predict(str(args.input))):
        result.save_to_json(save_path=str(args.output));result.save_to_markdown(save_path=str(args.output))
    (args.output/'runtime.json').write_text(json.dumps({'schema_version':1,'engine':'paddleocr','engine_version':'3.7.0','paddle_version':'3.2.1','pipeline_version':'v1.6','device':paddle.device.get_device(),'gpu_name':paddle.device.cuda.get_device_name(),'load_seconds':loaded-started,'total_seconds':time.perf_counter()-started,'quality_status':'not_verified'},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'status':'captured','elapsed_seconds':time.perf_counter()-started}),flush=True)
    return 0
if __name__=='__main__':raise SystemExit(main())
