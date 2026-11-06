"""Independent PNG/source-declaration and actual SVG/Markdown fidelity audit.

Inputs are explicitly selected CC0 author fixtures with manual semantic
transcriptions. This audit performs neither image recognition nor scientific
verification and never derives author expectations from the Rust compiler.
"""
from __future__ import annotations
import argparse
import copy
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import unicodedata
import xml.etree.ElementTree as ET

sys.path.insert(0,str(Path(__file__).resolve().parent))
from verify_formula import read, save, inventory, pins, png_info
from verify_extraction_knowledge import strict_json, fingerprint
from verify_knowledge_coordination import fields, exact, display_text, uuid_string, safe_name
from verify_knowledge_conflicts import guard_path
from visual_capability_registry import supports_svg_target

REPO=Path(__file__).resolve().parents[1]
FIXTURE=REPO/'fixtures/image-fidelity'
FROZEN_SHA='b74d28cc98598f53d5efbf47bacdab9aa80bb23f938ffb2537c3a2f7c072af1e'
CASE_FIELDS='schema_version purpose selection_id title source_kind image semantics dimensions readability'
SEMANTIC_FIELDS='schema_version producer author complete data_available caption conditions exceptions elements relations spatial_relations legend'
PROFILE_FIELDS='schema_version app app_version svg_supported local_images verified'
EXPECTED_FIELDS='schema_version producer author case_name scope scientific_quality knowledge_status published case semantics source_pins profile_name representation issue_codes fallback_reasons'
REPORT_FIELDS='schema_version producer scope request_id status preview_status knowledge_status scientific_quality published network_calls paid_calls case semantics profile checks decision plan_fingerprint inputs files artifacts'
PLAN_FIELDS='schema_version producer scope request_id case semantics decision'
COMMON=dict(schema_version=1,producer='knowledge_image',scope='selected_image_fidelity_v1',knowledge_status='pending',scientific_quality='not_run',published=False,network_calls=0,paid_calls=0)


def integer(value,minimum=0,maximum=2048):
    assert type(value) is int and minimum<=value<=maximum,'bounded integer geometry'


def identifier(value):
    assert type(value) is str and re.fullmatch(r'[a-z][a-z0-9_-]{0,63}',value),'bounded image identity'


def string(value,nullable=False,empty=False):
    if nullable and value is None:return
    assert type(value) is str and (empty and value=='' or value.strip()),'explicit source text'
    assert len(value.encode())<=1024 and not any(unicodedata.category(c)=='Cc' and c not in '\n\r\t' for c in value),'bounded literal source text'


def array(value,maximum=64):
    assert type(value) is list and len(value)<=maximum,'bounded source array'


def bbox(value,dimensions):
    assert type(value) is list and len(value)==4,'four-coordinate box'
    for n in value:integer(n)
    x,y,w,h=value;assert w>0 and h>0 and x+w<=dimensions[0] and y+h<=dimensions[1],'box within whole selected PNG'


def point(value,dimensions):
    assert type(value) is list and len(value)==2,'two-coordinate point'
    for n,limit in zip(value,dimensions):integer(n,0,limit)


def text_array(values):
    array(values,32)
    for v in values:string(v)
    assert len(set(values))==len(values),'duplicate source context'


def profile_schema(profile):
    fields(profile,PROFILE_FIELDS);exact(profile['schema_version'],1,'image profile version')
    for key in ('app','app_version'):string(profile[key])
    for key in ('svg_supported','local_images','verified'):assert type(profile[key]) is bool,'capability boolean'


def semantics_schema(s,dimensions):
    fields(s,SEMANTIC_FIELDS);exact(s['schema_version'],1,'semantic declaration version');exact(s['producer'],'author_image_declaration','manual declaration provenance')
    string(s['author']);string(s['caption'])
    for key in ('complete','data_available'):assert type(s[key]) is bool,'manual declaration boolean'
    for key in ('conditions','exceptions'):text_array(s[key])
    for key in ('elements','relations','spatial_relations','legend'):
        array(s[key],32 if key=='legend' else 64)
        identifiers=[]
        for item in s[key]:identifier(item['id']);identifiers.append(item['id'])
        assert len(set(identifiers))==len(identifiers),'unique identities within each declaration namespace'
    elements={e['id']:e for e in s['elements']};legends={l['id']:l for l in s['legend']}
    for e in s['elements']:
        fields(e,'id kind bbox label unit legend_id');assert e['kind'] in ('rect','circle','unsupported'),'finite declared element kind';bbox(e['bbox'],dimensions)
        if e['kind']=='circle':assert e['bbox'][2]==e['bbox'][3],'circle has square source box'
        string(e['label'],True);string(e['unit'],True)
        if e['legend_id'] is not None:identifier(e['legend_id']);assert e['legend_id'] in legends,'element legend identity'
    for l in s['legend']:
        fields(l,'id label meaning color bbox');bbox(l['bbox'],dimensions);string(l['label'],True);string(l['meaning'],True)
        assert type(l['color']) is str and re.fullmatch(r'#[0-9a-f]{6}',l['color']),'literal source color, no URLs'
    for r in s['relations']:
        fields(r,'id from to start end direction effect label unit legend_id')
        assert r['from'] in elements and r['to'] in elements and r['from']!=r['to'],'declared relation endpoints'
        point(r['start'],dimensions);point(r['end'],dimensions);assert r['start']!=r['end'],'nonzero relation vector'
        assert r['direction'] in ('forward','reverse','none','unknown'),'finite arrow direction'
        assert r['effect'] in ('positive','negative','neutral','unknown'),'explicit relation polarity'
        string(r['label'],True,True);string(r['unit'],True)
        if r['legend_id'] is not None:identifier(r['legend_id']);assert r['legend_id'] in legends,'relation legend identity'
    for r in s['spatial_relations']:
        fields(r,'id a b relation');assert r['a'] in elements and r['b'] in elements and r['a']!=r['b'],'declared spatial endpoints'
        assert r['relation'] in ('left_of','above','inside','overlaps','unknown'),'finite spatial relationship'


def selected_source(root):
    root=Path(root);guard_path(root,directory=True);raw=read(root/'case.json',16384);c=strict_json(raw);fields(c,CASE_FIELDS)
    exact(c['schema_version'],1,'selected image version');exact(c['purpose'],'image_fidelity','selected image purpose');identifier(c['selection_id']);string(c['title'])
    assert c['source_kind'] in ('diagram','spatial','experimental_curve','observation'),'image source kind'
    assert c['readability'] in ('clear','unreadable'),'author source legibility'
    assert type(c['dimensions']) is list and len(c['dimensions'])==2,'whole PNG dimensions'
    for dim in c['dimensions']:integer(dim,1)
    selected={};names=[]
    for key in ('image','semantics'):
        ref=c[key];fields(ref,'file sha256');safe_name(ref['file']);assert '/' not in ref['file'] and '\\' not in ref['file'],'explicit single file selection'
        assert type(ref['sha256']) is str and re.fullmatch('[a-f0-9]{64}',ref['sha256']),'source SHA'
        names.append(ref['file']);selected[key]=read(root/ref['file'],8*1024*1024 if key=='image' else 256*1024);exact(fingerprint(selected[key])['sha256'],ref['sha256'],'declared original '+key)
    assert len(set(names+['case.json']))==3,'selected filename collision'
    exact(set(inventory(root)),set(names)|{'case.json'},'exact three explicitly selected regular source files')
    exact(list(png_info(selected['image'])),c['dimensions'],'actual decoded PNG size')
    s=strict_json(selected['semantics']);semantics_schema(s,c['dimensions'])
    return c,s,{'source-case.json':raw,'source-semantics.json':selected['semantics'],'original.png':selected['image']}


def spatial_holds(r,elements):
    a,b=elements[r['a']],elements[r['b']];ax,ay,aw,ah=a['bbox'];bx,by,bw,bh=b['bbox'];kind=r['relation']
    if kind=='unknown':return True
    if kind=='left_of':return ax+aw<=bx
    if kind=='above':return ay+ah<=by
    ac=(ax+aw/2,ay+ah/2);bc=(bx+bw/2,by+bh/2)
    if kind=='inside':
        if b['kind']=='circle':
            radius=bw/2
            if a['kind']=='circle':return math.dist(ac,bc)+aw/2<=radius
            return all(math.dist(p,bc)<=radius for p in ((ax,ay),(ax+aw,ay),(ax,ay+ah),(ax+aw,ay+ah)))
        return ax>=bx and ay>=by and ax+aw<=bx+bw and ay+ah<=by+bh
    assert kind=='overlaps','explicit source spatial enum'
    if a['kind']=='circle' and b['kind']=='circle':return math.dist(ac,bc)<(aw+bw)/2
    if a['kind']=='circle' or b['kind']=='circle':
        circle,rect=(a,b) if a['kind']=='circle' else (b,a);cx,cy,cw,ch=circle['bbox'];rx,ry,rw,rh=rect['bbox'];center=(cx+cw/2,cy+ch/2);nearest=(min(max(center[0],rx),rx+rw),min(max(center[1],ry),ry+rh));return math.dist(center,nearest)<cw/2
    return max(ax,bx)<min(ax+aw,bx+bw) and max(ay,by)<min(ay+ah,by+bh)


def relation_geometry(r,elements):
    def interior(p,e):
        x,y,w,h=e['bbox']
        if e['kind']=='circle':return math.hypot(p[0]-x-w/2,p[1]-y-h/2)<w/2
        return x<p[0]<x+w and y<p[1]<y+h
    def border(p,e):
        x,y,w,h=e['bbox']
        if e['kind']=='circle':return abs(math.hypot(p[0]-x-w/2,p[1]-y-h/2)-w/2)<=1
        return x<=p[0]<=x+w and y<=p[1]<=y+h and (p[0] in (x,x+w) or p[1] in (y,y+h))
    a,b=r['start'],r['end'];source=elements[r['from']];target=elements[r['to']]
    if not border(a,source) or not border(b,target):return False
    dx=b[0]-a[0];dy=b[1]-a[1];length=math.hypot(dx,dy);step=min(length,2)/length
    return not interior([a[0]+step*dx,a[1]+step*dy],source) and not interior([b[0]-step*dx,b[1]-step*dy],target)


def classify(c,s,p):
    profile_schema(p);elements={e['id']:e for e in s['elements']};issues=[]
    if not s['complete']:issues.append('IMAGE_DECLARATION_INCOMPLETE')
    if not s['elements'] or any(e['label'] is None for e in s['elements']) or any(l['label'] is None or l['meaning'] is None for l in s['legend']):issues.append('IMAGE_UNKNOWN_LABEL')
    if any(r['label'] is None or r['direction']=='unknown' or r['effect']=='unknown' for r in s['relations']):issues.append('IMAGE_UNKNOWN_RELATION')
    if any(r['relation']=='unknown' for r in s['spatial_relations']):issues.append('IMAGE_UNKNOWN_SPATIAL')
    if any(not relation_geometry(r,elements) for r in s['relations']) or any(not spatial_holds(r,elements) for r in s['spatial_relations']):issues.append('IMAGE_GEOMETRY_MISMATCH')
    if c['readability']=='unreadable':issues.append('IMAGE_SOURCE_UNREADABLE')
    if not p['local_images']:issues.append('IMAGE_LOCAL_IMAGES_UNAVAILABLE')
    if issues:return None,issues,[]
    if c['source_kind']=='experimental_curve':return 'original_image',[],['IMAGE_CURVE_NO_DATA' if not s['data_available'] else 'IMAGE_CURVE_ORIGINAL']
    if c['source_kind']=='observation':return 'original_image',[],['IMAGE_OBSERVATION_ORIGINAL']
    fallback=[]
    if any(e['kind']=='unsupported' for e in s['elements']):fallback.append('IMAGE_GEOMETRY_UNSUPPORTED')
    if not supports_svg_target(p):fallback.append('IMAGE_TARGET_UNVERIFIED')
    if not p['svg_supported']:fallback.append('IMAGE_SVG_UNAVAILABLE')
    if not fallback:
        reason=layout(s,c['dimensions'])
        if reason:fallback.append(reason)
    return ('original_image' if fallback else 'svg_redraw'),[],fallback


def verify_fixture(root=FIXTURE):
    root=Path(root);frozen=read(root/'frozen-sha256.json');exact(fingerprint(frozen)['sha256'],FROZEN_SHA,'frozen independent author SHA')
    manifest=strict_json(frozen);actual=inventory(root);exact(set(actual),set(manifest['files'])|{'frozen-sha256.json'},'frozen author inventory');exact(pins({k:v for k,v in actual.items() if k!='frozen-sha256.json'}),manifest['files'],'all frozen author bytes')
    authors=strict_json(actual['authors.json'])
    for entry in authors['cases']:
        expected=strict_json(actual[entry['expected_file']]);fields(expected,EXPECTED_FIELDS)
        exact({k:expected[k] for k in ('schema_version','producer','author','scope','scientific_quality','knowledge_status','published')},dict(schema_version=1,producer='image_fidelity_expected',author='independent_self_authored_engineering_fixture',scope='selected_image_only_manual_declaration_not_recognition_or_science',scientific_quality='not_run',knowledge_status='pending',published=False),'independent expected provenance')
        c,s,_=selected_source(root/entry['case_directory']);exact(c,expected['case'],'frozen selection');exact(s,expected['semantics'],'frozen semantics');exact(pins(inventory(root/entry['case_directory'])),expected['source_pins'],'expected pins')
        result=classify(c,s,strict_json(actual[entry['profile_file']]));exact(result,(expected['representation'],expected['issue_codes'],expected['fallback_reasons']),'independent decision '+entry['name'])
    return authors


def expectation(path):
    path=Path(path);authors=verify_fixture(path.parent);assert any(e['expected_file']==path.name for e in authors['cases']),'selected frozen independent expected';return strict_json(read(path))


def svg_primitives(case,semantics):
    """Reconstruct the finite public rendering protocol from source geometry.

    These are author coordinates and visible field identities, independent of
    any saved plan/report. A polygon arrowhead is checked as real geometry.
    """
    result=[];legends={l['id']:l for l in semantics['legend']}
    def emit(tag,attrs,kind,field,value=None):result.append(dict(tag=tag,attrs=attrs,kind=kind,field=field,value=value))
    def text(id,x,y,value,kind,field):
        emit('text',dict(id=id,x=x,y=y,fill='#000000',**{'font-size':'14','font-family':'sans-serif','text-anchor':'middle'}),kind,field,value)
    w,h=case['dimensions'];emit('rect',dict(id='image-canvas',x=0,y=0,width=w,height=h,fill='#ffffff'),'canvas_background','case.dimensions')
    for i,e in sorted(enumerate(semantics['elements']),key=lambda pair:-(pair[1]['bbox'][2]*pair[1]['bbox'][3])):
        x,y,w,h=e['bbox'];attrs=dict(id='element-'+e['id'],fill=legends[e['legend_id']]['color'] if e['legend_id'] else '#ffffff',stroke='#000000',**{'stroke-width':'2'})
        if e['kind']=='rect':attrs.update(x=x,y=y,width=w,height=h);tag='rect'
        else:assert e['kind']=='circle','redraw cannot fabricate unsupported geometry';attrs.update(cx=x+w/2,cy=y+h/2,r=w/2);tag='circle'
        emit(tag,attrs,'element_shape',f'semantics.elements[{i}].bbox')
    for i,e in enumerate(semantics['elements']):
        x,y,w,h=e['bbox'];stem='element-'+e['id'];field=f'semantics.elements[{i}]'
        text(stem+'-label',x+w/2,y+h/2-8 if e['unit'] is not None else y+h/2+5,e['label'],'element_label',field+'.label')
        if e['unit'] is not None:text(stem+'-unit',x+w/2,y+h/2+10,e['unit'],'element_unit',field+'.unit')
    for i,l in enumerate(semantics['legend']):
        x,y,w,h=l['bbox'];stem='legend-'+l['id'];field=f'semantics.legend[{i}]'
        emit('rect',dict(id=stem,x=x,y=y,width=w,height=h,fill=l['color'],stroke='#000000',**{'stroke-width':'2'}),'legend_swatch',field+'.bbox')
        text(stem+'-label',x+w/2,y+h/2-8,l['label'],'legend_label',field+'.label');text(stem+'-meaning',x+w/2,y+h/2+10,l['meaning'],'legend_meaning',field+'.meaning')
    for i,r in enumerate(semantics['relations']):
        sx,sy=r['start'];ex,ey=r['end'];stem='relation-'+r['id'];field=f'semantics.relations[{i}]';color='#000000'
        emit('line',dict(id=stem,x1=sx,y1=sy,x2=ex,y2=ey,stroke=color,**{'stroke-width':'2'}),'relation_line',field)
        if r['direction']!='none':
            assert r['direction'] in ('forward','reverse'),'unknown arrow cannot redraw'
            tx,ty,ox,oy=(ex,ey,sx,sy) if r['direction']=='forward' else (sx,sy,ex,ey);dx=tx-ox;dy=ty-oy;length=math.hypot(dx,dy);ux=dx/length;uy=dy/length
            points=[(tx,ty),(tx-10*ux+5*uy,ty-10*uy-5*ux),(tx-10*ux-5*uy,ty-10*uy+5*ux)]
            emit('polygon',dict(id=stem+'-head',points=points,fill=color,stroke=color,**{'stroke-width':'2'}),'relation_head',field+'.direction')
        mx=(sx+ex)/2;my=(sy+ey)/2
        if r['effect']!='neutral':text(stem+'-effect',mx,my-28,{'positive':'+','negative':'−'}[r['effect']],'relation_effect',field+'.effect')
        text(stem+'-label',mx,my-10,r['label'],'relation_label',field+'.label')
        if r['unit'] is not None:text(stem+'-unit',mx,my+18,r['unit'],'relation_unit',field+'.unit')
    return result


def triangle_hits_element(points,element):
    """Actual positive-area triangle intersection; independent polygon clipping."""
    x,y,w,h=element['bbox']
    if element['kind']=='circle':
        center=(x+w/2,y+h/2);radius=w/2
        def cross(a,b,c):return (b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])
        signs=[cross(points[i],points[(i+1)%3],center) for i in range(3)]
        if all(v>=0 for v in signs) or all(v<=0 for v in signs):return True
        for i,a in enumerate(points):
            b=points[(i+1)%3];dx=b[0]-a[0];dy=b[1]-a[1];t=max(0,min(1,((center[0]-a[0])*dx+(center[1]-a[1])*dy)/(dx*dx+dy*dy)))
            if math.dist(center,(a[0]+t*dx,a[1]+t*dy))<radius:return True
        return False
    polygon=[tuple(p) for p in points]
    for axis,bound,lower in [(0,x,True),(0,x+w,False),(1,y,True),(1,y+h,False)]:
        if not polygon:return False
        clipped=[]
        def within(p):return p[axis]>=bound if lower else p[axis]<=bound
        for i,start in enumerate(polygon):
            end=polygon[(i+1)%len(polygon)];inside_start=within(start);inside_end=within(end)
            if inside_start:clipped.append(start)
            if inside_start!=inside_end:
                t=(bound-start[axis])/(end[axis]-start[axis]);clipped.append((start[0]+t*(end[0]-start[0]),start[1]+t*(end[1]-start[1])))
        polygon=clipped
    if len(polygon)<3:return False
    twice_area=abs(sum(a[0]*polygon[(i+1)%len(polygon)][1]-a[1]*polygon[(i+1)%len(polygon)][0] for i,a in enumerate(polygon)))
    return twice_area>1e-8


def layout(semantics,dimensions):
    """Conservative finite text/geometry protocol, not guessed source repair.

    Bounds derive from visible primitives and source geometry. Ambiguous font
    fit or overlap keeps the PNG rather than altering shape/arrow positions.
    """
    if any(any(int(color[offset:offset+2],16)<192 for offset in (1,3,5)) for color in (legend['color'] for legend in semantics['legend'])):
        return 'IMAGE_COLOR_CONTRAST'
    def width(value):
        total=0
        for c in value:
            total+=14 if c in 'WM' else 12 if c in 'wm' else 10 if 'A'<=c<='Z' else 9 if '0'<=c<='9' else 8 if c.isascii() else 28 if ord(c)>=0x1f000 else 14
        return total
    def simple(value):return value.strip()==value and not any(c in value for c in '\n\r\t') and not any(a.isspace() and b.isspace() for a,b in zip(value,value[1:]))
    def overlap(a,b):return max(a[0],b[0])<min(a[0]+a[2],b[0]+b[2]) and max(a[1],b[1])<min(a[1]+a[3],b[1]+b[3])
    def contains(outer,inner):return outer[0]<=inner[0] and outer[1]<=inner[1] and inner[0]+inner[2]<=outer[0]+outer[2] and inner[1]+inner[3]<=outer[1]+outer[3]
    def crosses(start,end,b):
        # Parametric open-box interval intersection with a 0.01px border guard.
        enter,leave=0.0,1.0
        for axis in (0,1):
            lower=b[axis]+.01;upper=b[axis]+b[axis+2]-.01;delta=end[axis]-start[axis]
            if delta==0:
                if not lower<=start[axis]<=upper:return False
            else:
                t1=(lower-start[axis])/delta;t2=(upper-start[axis])/delta;enter=max(enter,min(t1,t2));leave=min(leave,max(t1,t2))
                if enter>leave:return False
        return enter<=leave
    elements=semantics['elements'];legends=semantics['legend']
    for e in elements:
        x,y,w,h=e['bbox'];capacity=w*.75 if e['kind']=='circle' else w
        values=[e['label']]+([e['unit']] if e['unit'] is not None else [])
        if any(not simple(v) or width(v)+12>capacity for v in values) or h<(44 if e['unit'] is not None else 28):return 'IMAGE_TEXT_LAYOUT'
    for l in legends:
        if any(not simple(v) or width(v)+12>l['bbox'][2] for v in (l['label'],l['meaning'])) or l['bbox'][3]<44:return 'IMAGE_TEXT_LAYOUT'
        if any(overlap(e['bbox'],l['bbox']) for e in elements):return 'IMAGE_GEOMETRY_LAYOUT'
    for i,e in enumerate(elements):
        for other in elements[i+1:]:
            if overlap(e['bbox'],other['bbox']) and not contains(e['bbox'],other['bbox']) and not contains(other['bbox'],e['bbox']):return 'IMAGE_GEOMETRY_LAYOUT'
    if any(overlap(a['bbox'],b['bbox']) for i,a in enumerate(legends) for b in legends[i+1:]):return 'IMAGE_GEOMETRY_LAYOUT'
    for i,r in enumerate(semantics['relations']):
        length=math.dist(r['start'],r['end'])
        if length<20:return 'IMAGE_GEOMETRY_LAYOUT'
        if any(crosses(r['start'],r['end'],e['bbox']) for e in elements if e['id'] not in (r['from'],r['to'])) or any(crosses(r['start'],r['end'],l['bbox']) for l in legends):return 'IMAGE_GEOMETRY_LAYOUT'
        if r['direction'] in ('forward','reverse'):
            head=next(p['attrs']['points'] for p in svg_primitives(dict(dimensions=dimensions),semantics) if p['kind']=='relation_head' and p['field']==f'semantics.relations[{i}].direction')
            def stroke_box(item):
                x,y,w,h=item['bbox'];return dict(kind='rect',bbox=[x-1,y-1,w+2,h+2])
            if any(triangle_hits_element(head,stroke_box(e)) for e in elements if e['id'] not in (r['from'],r['to'])) or any(triangle_hits_element(head,stroke_box(l)) for l in legends):return 'IMAGE_GEOMETRY_LAYOUT'
        values=[r['label']]+([r['unit']] if r['unit'] is not None else [])+({'positive':['+'],'negative':['−'],'neutral':[]}[r['effect']])
        if any(not simple(v) or width(v)+12>length-20 for v in values if v):return 'IMAGE_TEXT_LAYOUT'
    primitives=svg_primitives(dict(dimensions=dimensions),semantics);text_boxes=[]
    for primitive in primitives:
        if primitive['tag']=='polygon':
            if any(p[0]<0 or p[1]<0 or p[0]>dimensions[0] or p[1]>dimensions[1] for p in primitive['attrs']['points']):return 'IMAGE_GEOMETRY_LAYOUT'
        if primitive['tag']!='text' or not primitive['value']:continue
        a=primitive['attrs'];w=width(primitive['value'])+12;bounds=[a['x']-w/2,a['y']-14,w,18];owner=None
        if primitive['kind'].startswith('element_'):
            index=int(re.search(r'elements\[(\d+)\]',primitive['field'])[1]);owner=elements[index]
        text_boxes.append(bounds)
        if bounds[0]<0 or bounds[1]<0 or bounds[0]+bounds[2]>dimensions[0] or bounds[1]+bounds[3]>dimensions[1]:return 'IMAGE_TEXT_LAYOUT'
        for e in elements:
            if e is owner:continue
            if overlap(bounds,e['bbox']) and not (owner and contains(e['bbox'],owner['bbox'])):return 'IMAGE_GEOMETRY_LAYOUT'
    if any(overlap(a,b) for i,a in enumerate(text_boxes) for b in text_boxes[i+1:]):return 'IMAGE_GEOMETRY_LAYOUT'
    return None


def svg_nodes(raw):
    assert len(raw)<=2*1024*1024,'bounded actual SVG';decoded=raw.decode('utf-8')
    assert '<!' not in decoded and '<?' not in decoded,'no SVG external entities, declarations or instructions'
    root=ET.fromstring(decoded);assert root.tag=='{http://www.w3.org/2000/svg}svg','real SVG namespace'
    nodes=[];patterns=list(re.finditer(rb'<(?:rect|circle|line|polygon)\b[^<>]*\s*/>|<text\b[^<>]*>[^<>]*</text>',raw))
    children=list(root);assert len(patterns)==len(children),'each real primitive has exactly one recorded source byte range'
    assert all(child.tag.split('}')[-1] in ('rect','circle','line','polygon','text') and not list(child) for child in children),'finite actual SVG primitives, no hidden groups/scripts'
    for child,match in zip(children,patterns):
        isolated=ET.fromstring(match[0].decode());assert isolated.tag==child.tag.split('}')[-1] and isolated.attrib==child.attrib and isolated.text==child.text,'primitive lexical and XML order agree'
        assert child.tail is None or child.tail.strip()=='','no stray SVG text'
        nodes.append(dict(tag=child.tag.split('}')[-1],attrs=child.attrib,value=child.text,start=match.start(),end=match.end()))
    assert root.text is None or root.text.strip()=='','no root SVG stray text'
    return root,nodes


def verify_svg(raw,case,semantics,file_report):
    safety=layout(semantics,case['dimensions'])
    assert safety is None,'actual SVG violates independent source layout/color safety: '+str(safety)
    root,actual=svg_nodes(raw);w,h=case['dimensions'];exact(root.attrib,dict(width=str(w),height=str(h),viewBox=f'0 0 {w} {h}'),'actual original coordinate system')
    wanted=svg_primitives(case,semantics);assert len(actual)==len(wanted),'complete actual element/label/arrow/legend denominator'
    mappings=[]
    for node,expected in zip(actual,wanted):
        exact(node['tag'],expected['tag'],'actual source shape type');exact(set(node['attrs']),set(expected['attrs']),'no active, hidden or unverified SVG attribute')
        for key,value in expected['attrs'].items():
            observed=node['attrs'][key]
            if key=='points':
                assert re.fullmatch(r'-?[0-9]+(?:\.[0-9]+)?,-?[0-9]+(?:\.[0-9]+)?(?: -?[0-9]+(?:\.[0-9]+)?,-?[0-9]+(?:\.[0-9]+)?){2}',observed),'three explicit arrowhead points'
                pairs=[tuple(float(n) for n in p.split(',')) for p in observed.split()]
                def matches(order):return all(abs(a-b)<=0.00051 for pair,target in zip(pairs,order) for a,b in zip(pair,target))
                assert matches(value) or matches([value[0],value[2],value[1]]),'actual arrowhead direction/shape from original endpoints'
            elif type(value) in (int,float):
                assert re.fullmatch(r'[0-9]+(?:\.[0-9]+)?',observed) and abs(float(observed)-value)<=0.00051,'actual primitive source geometry '+key
            else:exact(observed,value,'actual primitive literal '+key)
        exact(node['value'] if node['value'] is not None else ('' if node['tag']=='text' else None),expected['value'],'actual label/effect/unit/legend field meaning')
        mappings.append(dict(kind=expected['kind'],source_field=expected['field'],start_byte=node['start'],end_byte=node['end']))
    exact(file_report['mappings'],mappings,'actual SVG primitive-to-source mapping denominator')
    return actual


FALLBACK_TEXT={
 'IMAGE_GEOMETRY_UNSUPPORTED':'来源含本编译器尚不能保真的图形，使用完整原图。',
 'IMAGE_CURVE_NO_DATA':'未提供原始实验数据，保留观测曲线原图，不生成精确曲线。',
 'IMAGE_CURVE_ORIGINAL':'实验曲线使用原图；本入口不从图像或数据重建曲线。',
 'IMAGE_OBSERVATION_ORIGINAL':'图像承载原始观测，保留完整原图。',
 'IMAGE_TARGET_UNVERIFIED':'目标SVG能力尚未实测，使用本地原图。',
 'IMAGE_SVG_UNAVAILABLE':'目标未启用SVG，使用本地原图。',
 'IMAGE_TEXT_LAYOUT':'标签的空白、长度或位置不能在有限重绘中完整保留，使用原图。',
 'IMAGE_GEOMETRY_LAYOUT':'有限重绘可能遮蔽图形、标签或箭头，使用原图。',
 'IMAGE_COLOR_CONTRAST':'来源颜色与固定黑字不能在有限重绘中保证可读，保留原图。',
}


def markdown_nodes(raw):
    raw.decode('utf-8');nodes=[];at=0
    while at<len(raw):
        end=raw.find(b'\n',at);assert end>=0,'complete Markdown line';line=raw[at:end]
        if not line:at=end+1;continue
        fence=re.fullmatch(rb'(`{3,})text',line)
        if fence:
            start=end+1;terminal=b'\n'+fence[1]+b'\n';finish=raw.find(terminal,start);assert finish>=0,'source literal fence closes'
            nodes.append(dict(kind='literal',value=raw[start:finish].decode(),start=start,end=finish));at=finish+len(terminal);continue
        heading=re.fullmatch(rb'(#{1,3}) (.+)',line)
        if heading:nodes.append(dict(kind='heading',value=heading[2].decode(),level=len(heading[1]),start=at,end=end))
        elif re.fullmatch(rb'!\[\[(?:original\.png|redrawn\.svg)\]\]',line):nodes.append(dict(kind='image',value=line.decode(),start=at,end=end))
        elif line=='[原始图像](original.png)'.encode():nodes.append(dict(kind='link',value=line.decode(),start=at,end=end))
        else:
            label=line.decode();assert label=='选定来源未列出。' or re.fullmatch(r'[\u4e00-\u9fff]+：',label),'unknown visible Markdown node'
            nodes.append(dict(kind='label',value=label,start=at,end=end))
        at=end+1
    return nodes


def note_semantic_nodes(c,s,decision):
    wanted=[]
    def node(kind,value,level=None,mapping=None):
        item=dict(kind=kind,value=value)
        if level is not None:item['level']=level
        if mapping:item['mapping']=mapping
        wanted.append(item)
    def heading(value,level=2):node('heading',value,level)
    def literal(value,kind,field):node('literal',value,mapping=(kind,field))
    def field(label,value,kind,path):node('label',label+'：');literal(value,kind,path)
    def coordinates(value):return json.dumps(value,separators=(',',':'))
    heading('图像预览',1);heading('标题');literal(c['title'],'title','case.title');heading('图像')
    if decision['representation']=='svg_redraw':node('image','![[redrawn.svg]]',mapping=('image_embed','semantics'))
    else:node('image','![[original.png]]',mapping=('image_embed','case.image'))
    for i,reason in enumerate(decision['fallback_reasons']):literal(FALLBACK_TEXT[reason],'fallback_reason',f'decision.fallback_reasons[{i}]')
    heading('图意');literal(s['caption'],'caption','semantics.caption');heading('元素')
    element_fields=[('标识','id','element_id'),('图形','kind','element_kind'),('标签','label','element_label'),('单位','unit','element_unit'),('图例','legend_id','element_legend'),('原图坐标','bbox','element_geometry')]
    relation_fields=[('标识','id','relation_id'),('起点元素','from','relation_from'),('终点元素','to','relation_to'),('起点坐标','start','relation_start'),('终点坐标','end','relation_end'),('箭头方向','direction','relation_direction'),('正负作用','effect','relation_effect'),('标签','label','relation_label'),('单位','unit','relation_unit'),('图例','legend_id','relation_legend')]
    spatial_fields=[('标识','id','spatial_relation'),('对象','a','spatial_relation'),('参照对象','b','spatial_relation'),('相对位置','relation','spatial_relation')]
    legend_fields=[('标识','id','legend_id'),('标签','label','legend_label'),('含义','meaning','legend_meaning'),('颜色','color','legend_color'),('原图坐标','bbox','legend_geometry')]
    for collection,title,items in [('elements','元素',element_fields),('relations','关系',relation_fields),('spatial_relations','空间关系',spatial_fields),('legend','图例',legend_fields)]:
        if collection!='elements':heading(title)
        for i,item in enumerate(s[collection]):
            heading(title+' '+str(i+1),3)
            for label,key,kind in items:
                value=item[key]
                if key in ('bbox','start','end'):value=coordinates(value)
                elif value is None:value='来源未给出单位' if key=='unit' else '来源未关联图例'
                field(label,value,kind,f'semantics.{collection}[{i}].{key}')
    for title,key in [('适用条件','conditions'),('例外','exceptions')]:
        heading(title)
        if not s[key]:node('label','选定来源未列出。')
        for i,value in enumerate(s[key]):literal(value,key,f'semantics.{key}[{i}]')
    heading('原始图像');node('link','[原始图像](original.png)',mapping=('attachment_link','case.image'))
    return wanted


def verify_note(raw,case,semantics,decision,file_report):
    actual=markdown_nodes(raw);wanted=note_semantic_nodes(case,semantics,decision)
    assert len(actual)==len(wanted),'complete visible source sections and field/value associations'
    mappings=[]
    for observed,expected in zip(actual,wanted):
        for key in ('kind','value','level'):
            exact(observed.get(key),expected.get(key),'actual field-to-value and semantic section association '+key)
        if expected.get('mapping'):
            kind,field=expected['mapping'];mappings.append(dict(kind=kind,source_field=field,start_byte=observed['start'],end_byte=observed['end']))
    exact(file_report['mappings'],mappings,'actual Markdown content-to-source field positions')


def verify_output(case_root,profile_path,vault,expected_path,target_profile=None):
    """Optional target is a selected external original; author meaning stays frozen."""
    expected=expectation(expected_path);c,s,source=selected_source(case_root)
    exact(c,expected['case'],'source selection is frozen author original');exact(s,expected['semantics'],'source semantics are frozen independent declarations');exact(pins(inventory(case_root)),expected['source_pins'],'author source original bytes')
    profile_raw=read(profile_path);profile=strict_json(profile_raw);profile_schema(profile)
    exact(profile,strict_json(read(Path(expected_path).parent/('profile-'+expected['profile_name']+'.json'))),'selected frozen target profile')
    decision=dict(representation=expected['representation'],fallback_reasons=expected['fallback_reasons'],issue_codes=expected['issue_codes'])
    if target_profile is not None:
        profile_raw=read(target_profile);profile=strict_json(profile_raw);profile_schema(profile)
        representation,issues,fallback=classify(c,s,profile)
        decision=dict(representation=representation,fallback_reasons=fallback,issue_codes=issues)
    source['profile.json']=profile_raw;actual=inventory(vault)
    for name,raw in source.items():exact(actual[name],raw,'actual original source bytes preserved '+name)
    report=strict_json(actual['image.json']);fields(report,REPORT_FIELDS)
    for key,value in COMMON.items():exact(report[key],value,'unpublished manual engineering scope '+key)
    uuid_string(report['request_id'])
    for key,value in [('case',c),('semantics',s),('profile',profile)]:exact(report[key],value,'report source binding '+key)
    exact(report['decision'],decision,'source-grounded image outcome')
    ready=decision['representation'] is not None;exact(report['status'],'passed' if ready else 'needs_review','true semantic status');exact(report['preview_status'],'rendered' if ready else 'not_run','true affected preview status')
    checks=report['checks'];fields(checks,'source_preservation author_declaration geometry_review semantic_review note_plan rendering saved_output')
    for key in ('source_preservation','saved_output'):exact(checks[key],'passed','actual preserved output '+key)
    unknown=not s['complete'] or not s['elements'] or any(e['label'] is None for e in s['elements']) or any(r['label'] is None or r['direction']=='unknown' or r['effect']=='unknown' for r in s['relations']) or any(r['relation']=='unknown' for r in s['spatial_relations']) or any(l['label'] is None or l['meaning'] is None for l in s['legend'])
    exact(checks['author_declaration'],'needs_review' if unknown else 'passed','all and only manually confirmed declaration fields')
    elements={e['id']:e for e in s['elements']};geometry=all(relation_geometry(r,elements) for r in s['relations']) and all(spatial_holds(r,elements) for r in s['spatial_relations'])
    exact(checks['geometry_review'],'passed' if geometry else 'needs_review','actual independent endpoint/spatial geometry')
    exact(checks['semantic_review'],'passed' if ready else 'needs_review','no false semantic success')
    for key in ('note_plan','rendering'):exact(checks[key],'passed' if ready else 'not_run','blocked expression cannot claim '+key)
    exact(report['inputs'],pins(source),'all input fingerprints')
    exact(strict_json(actual['.owner.json']),dict(schema_version=1,producer='knowledge_image',request_id=report['request_id'],purpose='unpublished_image'),'owned new test Vault')
    wanted_inventory=set(source)|{'.owner.json','image.json','image.md'}
    if ready:
        wanted_inventory|={'plan.json','图像预览.md'}
        plan=strict_json(actual['plan.json']);fields(plan,PLAN_FIELDS);exact(plan,dict(schema_version=1,producer='knowledge_image',scope='selected_image_fidelity_v1',request_id=report['request_id'],case=c,semantics=s,decision=decision),'plan fully binds independent author source meaning');exact(report['plan_fingerprint'],fingerprint(actual['plan.json']),'saved plan bytes')
        names=['图像预览.md']
        if decision['representation']=='svg_redraw':wanted_inventory.add('redrawn.svg');names.append('redrawn.svg')
        exact([f['relative_path'] for f in report['files']],names,'actual rendered file denominator/order')
        for f in report['files']:
            fields(f,'relative_path content_sha256 byte_length mappings');raw=actual[f['relative_path']];exact(f['content_sha256'],fingerprint(raw)['sha256'],'actual rendered content SHA');exact(f['byte_length'],len(raw),'actual rendered file length')
            if f['relative_path']=='redrawn.svg':verify_svg(raw,c,s,f)
            else:verify_note(raw,c,s,decision,f)
    else:exact(report['plan_fingerprint'],None,'blocked case has no successful plan');exact(report['files'],[],'blocked image has no preview files')
    exact(set(actual),wanted_inventory,'no fake redraw/curve/data or unowned output');exact(report['artifacts'],pins({k:v for k,v in actual.items() if k not in ('image.json','image.md')}),'complete saved artifact inventory')
    markdown=actual['image.md'].decode();matches=re.findall(r'```json\n(.*?)\n```',markdown,re.S);assert len(matches)==1,'single readable report payload';exact(strict_json(matches[0].encode()),report,'actual dual report agreement')
    outside=re.sub(r'```json\n.*?\n```','<checked-report-payload>',markdown,flags=re.S)
    wanted='# 选定图像保真预览报告\n\n状态：'+report['status']+'；预览：'+report['preview_status']+'；知识：pending；科学质量：not_run；发布：false。\n\n仅选定图像和手工结构声明工程核对；不执行自动图像理解；网络与付费调用：0。\n\n<checked-report-payload>\n'
    exact(outside,wanted,'whole visible report shell including honest scope/status')
    return dict(status='passed',case_name=expected['case_name'],representation=decision['representation'],original_bytes_unchanged=True,issue_codes=decision['issue_codes'],scientific_quality='not_run')


def resynchronize_result(vault):
    """Attacker updates ordinary fingerprints and both reports after tampering."""
    vault=Path(vault);report=strict_json(read(vault/'image.json'))
    if (vault/'plan.json').exists():report['plan_fingerprint']=fingerprint(read(vault/'plan.json'))
    for entry in report['files']:
        raw=read(vault/entry['relative_path']);entry.update(content_sha256=fingerprint(raw)['sha256'],byte_length=len(raw))
    actual=inventory(vault);report['artifacts']=pins({k:v for k,v in actual.items() if k not in ('image.json','image.md')});save(vault/'image.json',report)
    wire=read(vault/'image.json').decode().rstrip('\n');md=read(vault/'image.md').decode();md=re.sub(r'```json\n.*?\n```',lambda _:'```json\n'+wire+'\n```',md,flags=re.S)
    md=re.sub(r'^状态：[^\n]+$',lambda _:'状态：'+report['status']+'；预览：'+report['preview_status']+'；知识：'+report['knowledge_status']+'；科学质量：'+report['scientific_quality']+'；发布：'+str(report['published']).lower()+'。',md,flags=re.M);(vault/'image.md').write_text(md,encoding='utf-8')


def mutate_svg(vault,identifier,change):
    """Rebind every surviving SVG byte mapping after a real primitive change."""
    vault=Path(vault);path=vault/'redrawn.svg';raw=read(path);_,old=svg_nodes(raw);selected=[n for n in old if n['attrs']['id']==identifier];assert len(selected)==1,'attack targets a real baseline primitive'
    node=selected[0];replacement=change(raw[node['start']:node['end']]);new_raw=raw[:node['start']]+replacement+raw[node['end']:];path.write_bytes(new_raw);_,new=svg_nodes(new_raw)
    positions={n['attrs']['id']:n for n in new};report=strict_json(read(vault/'image.json'));entry=next(f for f in report['files'] if f['relative_path']=='redrawn.svg');mappings=[]
    for mapping in entry['mappings']:
        matched=[n for n in old if (n['start'],n['end'])==(mapping['start_byte'],mapping['end_byte'])];assert len(matched)==1,'real original mapping before attack';id=matched[0]['attrs']['id']
        if id in positions:mapping.update(start_byte=positions[id]['start'],end_byte=positions[id]['end']);mappings.append(mapping)
    entry['mappings']=mappings;save(vault/'image.json',report)


def mutate_note(vault,old,new):
    vault=Path(vault);p=vault/'图像预览.md';raw=read(p);assert old.encode() in raw,'attack changes actual visible source text';altered=raw.replace(old.encode(),new.encode(),1);p.write_bytes(altered)
    before=markdown_nodes(raw);after=markdown_nodes(altered);assert len(before)==len(after),'field association attack keeps all nodes'
    report=strict_json(read(vault/'image.json'));entry=next(f for f in report['files'] if f['relative_path']=='图像预览.md')
    for mapping in entry['mappings']:
        indices=[i for i,n in enumerate(before) if n['start']==mapping['start_byte'] and n['end']==mapping['end_byte']];assert len(indices)==1,'real baseline mapped source node';n=after[indices[0]];mapping.update(start_byte=n['start'],end_byte=n['end'])
    save(vault/'image.json',report)


def misassociate_note_fields(vault):
    vault=Path(vault);path=vault/'图像预览.md';raw=read(path);unit='单位：'.encode();legend='图例：'.encode();altered=raw.replace(unit,b'__TEMP_LABEL__',1).replace(legend,unit,1).replace(b'__TEMP_LABEL__',legend,1);path.write_bytes(altered)
    old=markdown_nodes(raw);new=markdown_nodes(altered);exact(sorted(n['value'] for n in old),sorted(n['value'] for n in new),'attack preserves flattened labels and values')
    report=strict_json(read(vault/'image.json'));entry=next(f for f in report['files'] if f['relative_path']=='图像预览.md')
    for m in entry['mappings']:
        i=next(i for i,n in enumerate(old) if n['start']==m['start_byte'] and n['end']==m['end_byte']);m.update(start_byte=new[i]['start'],end_byte=new[i]['end'])
    save(vault/'image.json',report)


def run_audit(cli,root):
    cli,root=Path(cli),Path(root);assert cli.is_absolute() and root.is_absolute(),'absolute selected CLI and test root';guard_path(cli,directory=False);guard_path(root.parent,directory=True);assert not root.exists(),'brand new audit root'
    authors=verify_fixture();root.mkdir();frozen=root/'frozen-authors';shutil.copytree(FIXTURE,frozen)
    for name in ('commands','vaults','mutations','guards','source-pins'):(root/name).mkdir()
    source_files=['scripts/verify_image.py','tests/test_verify_image.py']+[str(p.relative_to(REPO)) for p in sorted((REPO/'crates/noetome-cli/src').glob('image*.rs'))]
    for name in source_files:
        path=root/'source-pins'/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(read(REPO/name))
    sentinel=root/'external-sentinel.txt';sentinel.write_bytes(b'User-owned image audit sentinel.\r\n')
    baseline=dict(cli=str(cli),cli_fingerprint=fingerprint(read(cli,256*1024*1024)),python=sys.executable,platform=sys.platform,fixture_manifest=fingerprint(read(frozen/'frozen-sha256.json')),source_pins=pins(inventory(root/'source-pins')),sentinel=fingerprint(read(sentinel)))
    commands=[];scenarios=[]
    def record(status='running'):save(root/'audit.json',dict(status=status,before=baseline,commands=commands,scenarios=scenarios))
    record()
    def capture(path):
        path=Path(path)
        if path.is_dir():return pins(inventory(path))
        if sys.platform!='win32' and path.stat().st_nlink>1:
            guard_path(path.parent,directory=True);assert not path.is_symlink() and path.is_file(),'known created hardlink';return fingerprint(path.read_bytes())
        return fingerprint(read(path))
    def command(label,args,codes,protected=()):
        before={str(p):capture(p) for p in protected};before_sentinel=read(sentinel);argv=[str(cli),*map(str,args)];start=time.monotonic();result=subprocess.run(argv,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=60)
        prefix=root/'commands'/(str(len(commands)).zfill(3)+'-'+label);prefix.with_suffix('.stdout.bin').write_bytes(result.stdout);prefix.with_suffix('.stderr.bin').write_bytes(result.stderr);after={str(p):capture(p) for p in protected}
        receipt=dict(label=label,argv=argv,exit_code=result.returncode,elapsed_seconds=time.monotonic()-start,stdout=fingerprint(result.stdout),stderr=fingerprint(result.stderr),protected_before=before,protected_after=after,sentinel_before=fingerprint(before_sentinel),sentinel_after=fingerprint(read(sentinel)));save(prefix.with_suffix('.receipt.json'),receipt);commands.append(receipt);record()
        exact(before,after,'protected originals and existing target '+label);exact(before_sentinel,read(sentinel),'external sentinel '+label);assert result.returncode in codes,label+' exit='+str(result.returncode)+' '+result.stderr.decode(errors='replace');return result
    def rejected(label,operation):
        try:operation()
        except (AssertionError,ValueError,KeyError,OSError,ET.ParseError) as failure:scenarios.append(dict(name=label,status='rejected',reason=str(failure),exception=type(failure).__name__));record();return
        raise AssertionError('independent checker accepted synchronized mutation '+label)
    by_name={entry['name']:entry for entry in authors['cases']}
    for entry in authors['cases']:
        name=entry['name'];source=frozen/entry['case_directory'];profile=frozen/entry['profile_file'];expected=frozen/entry['expected_file'];target=root/'vaults'/name;gold=strict_json(read(expected))
        result=command(name,['knowledge','image',source,profile,target],{0} if gold['representation'] else {3},(source,profile));checked=verify_output(source,profile,target,expected);exact(strict_json(result.stdout),strict_json(read(target/'image.json')),'actual ordinary CLI stdout equals saved report');scenarios.append(dict(name=name,status='passed',verification=checked));record()
    def mutate(label,operation,source_name='signed_arrows'):
        target=root/'mutations'/label;shutil.copytree(root/'vaults'/source_name,target);operation(target);resynchronize_result(target);entry=by_name[source_name];rejected(label,lambda:verify_output(frozen/entry['case_directory'],frozen/entry['profile_file'],target,frozen/entry['expected_file']))
    for label,id,old,new in [
      ('svg-canvas-black','image-canvas',b'fill="#ffffff"',b'fill="#000000"'),
      ('svg-label-replaced','element-feed-label',b'Feed',b'Flee'),
      ('svg-unit-loss','element-feed-unit',b'mmol/L',b'kg'),
      ('svg-effect-flip','relation-gain-effect',b'>+</text>',b'>\xe2\x88\x92</text>'),
      ('svg-legend-meaning','legend-positive-meaning',b'positive activation',b'negative inhibition'),
      ('svg-position-moved','element-feed',b'x="40"',b'x="400"'),
      ('svg-relation-reversed','relation-gain',b'x1="260" y1="210" x2="390"',b'x1="390" y1="210" x2="260"'),
      ('svg-arrow-reversed','relation-gain-head',b'390.000,210.000 380.000,215.000 380.000,205.000',b'260.000,210.000 270.000,215.000 270.000,205.000'),
      ('svg-arrow-light-ink','relation-gain',b'stroke="#000000"',b'stroke="#c5efcb"'),
      ('svg-head-light-ink','relation-gain-head',b'fill="#000000"',b'fill="#c5efcb"'),
      ('svg-hidden-text','element-feed-label',b'fill="#000000"',b'fill="#ffffff"')]:
        def attack(target,id=id,old=old,new=new):
            def change(raw):assert old in raw,'real SVG mutation baseline';return raw.replace(old,new,1)
            mutate_svg(target,id,change)
        mutate(label,attack)
    for label,id in [('svg-canvas-deleted','image-canvas'),('svg-node-deleted','element-feed'),('svg-label-deleted','element-feed-label'),('svg-arrow-deleted','relation-gain-head'),('svg-unit-deleted','relation-gain-unit'),('svg-legend-deleted','legend-positive-meaning')]:mutate(label,lambda t,id=id:mutate_svg(t,id,lambda raw:b''))
    mutate('spatial-containment-lost',lambda t:mutate_svg(t,'element-inner',lambda raw:raw.replace(b'cx="760.000"',b'cx="240.000"')),'spatial')
    for label,old,new in [('note-effect-flip','```text\npositive\n```','```text\nnegative\n```'),('note-unit-loss','```text\nmmol/L\n```','```text\nkg\n```'),('note-legend-assignment','```text\npositive\n```','```text\nnegative\n```'),('note-caption-loss','Fictional engineering diagram; manual source declaration, no image recognition.','A scientific result.'),('note-condition-loss','Only the selected schematic relationships are asserted.','No condition.'),('note-exception-loss','No scientific accuracy or measured data is claimed.','All data measured.')]:mutate(label,lambda t,old=old,new=new:mutate_note(t,old,new))
    mutate('note-unit-legend-field-association',misassociate_note_fields)
    def false_shell(t):
        p=t/'image.md';raw=read(p);p.write_bytes(raw.replace('不执行自动图像理解'.encode(),'所有图像已自动识别，科学质量passed'.encode(),1))
    mutate('false-visible-report-shell',false_shell)
    mutate('wrong-local-image-link',lambda t:mutate_note(t,'![[original.png]]','![[redrawn.svg]]'),'unsupported_geometry')
    def image_replace(t):(t/'original.png').write_bytes(read(frozen/'source_unreadable/source.png'))
    mutate('preserved-original-image-replaced',image_replace)
    def fake_curve(t):(t/'fake-curve.csv').write_bytes(b'time,response\n0,0\n1,100\n')
    mutate('fabricated-curve-data',fake_curve,'curve_no_data')
    def fake_success(t):
        report=strict_json(read(t/'image.json'));report.update(status='passed',preview_status='rendered');report['decision'].update(representation='original_image',issue_codes=[]);save(t/'image.json',report)
    for name in ('unknown_label','unknown_relation','unknown_spatial','source_unreadable','images_unavailable'):mutate('false-success-'+name,fake_success,name)
    def fake_science(t):
        report=strict_json(read(t/'image.json'));report.update(scientific_quality='passed',published=True);save(t/'image.json',report)
    mutate('false-scientific-publication',fake_science)
    def false_check(t):
        report=strict_json(read(t/'image.json'));report['checks']['author_declaration']='passed';save(t/'image.json',report)
    mutate('false-author-check',false_check,'unknown_label')
    native=frozen/'signed_arrows';profile=frozen/'profile-native.json';expected=frozen/'expected-signed_arrows.json'
    # Real source/plan/output/dual-report/hash synchronized changes are compiled
    # as new declarations, then refused against unchanged frozen author truth.
    for label,operation in [('source-node-deleted',lambda s:(s['elements'].pop(2),s['relations'].pop(),s['spatial_relations'].pop())),('source-arrow-reverse',lambda s:s['relations'][0].update(direction='reverse')),('source-effect-flip',lambda s:s['relations'][0].update(effect='negative')),('source-unit-loss',lambda s:s['elements'][0].update(unit=None)),('source-legend-loss',lambda s:s['relations'][0].update(legend_id=None)),('source-spatial-change',lambda s:s['elements'][0].update(bbox=[0,150,220,120]))]:
        folder=root/'guards'/label;folder.mkdir();source=folder/'case';shutil.copytree(native,source);s=strict_json(read(source/'semantics.json'));operation(s);save(source/'semantics.json',s);c=strict_json(read(source/'case.json'));c['semantics']['sha256']=fingerprint(read(source/'semantics.json'))['sha256'];save(source/'case.json',c);target=folder/'vault';command(label,['knowledge','image',source,profile,target],{0,3},(source,profile));rejected(label,lambda source=source,target=target:verify_output(source,profile,target,expected))
    # Supplemental declaration fault probes save an independent expected decision
    # and source pins before calling the compiler. They do not add author cases.
    def edge(label,change,case_change=None,profile_change=None):
        folder=root/'guards'/label;folder.mkdir();source=folder/'case';shutil.copytree(native,source);sem=strict_json(read(source/'semantics.json'));change(sem);save(source/'semantics.json',sem)
        c=strict_json(read(source/'case.json'));c['semantics']['sha256']=fingerprint(read(source/'semantics.json'))['sha256']
        if case_change:case_change(c)
        save(source/'case.json',c);p=strict_json(read(profile))
        if profile_change:profile_change(p)
        edge_profile=folder/'profile.json';save(edge_profile,p);semantics_schema(sem,c['dimensions']);rep,issues,fallback=classify(c,sem,p)
        before=dict(purpose='supplemental_declaration_fault_probe_not_new_author_case',source_pins=pins(inventory(source)),profile=fingerprint(read(edge_profile)),decision=dict(representation=rep,issue_codes=issues,fallback_reasons=fallback));save(folder/'independent-before.json',before)
        target=folder/'vault';result=command(label,['knowledge','image',source,edge_profile,target],{0} if rep else {3},(source,edge_profile));report=strict_json(read(target/'image.json'));exact(strict_json(result.stdout),report,'fault probe stdout and report');exact(report['decision'],before['decision'],'independent pre-call bounded declaration decision '+label);exact(report['checks']['geometry_review'],'needs_review' if 'IMAGE_GEOMETRY_MISMATCH' in issues else 'passed','independent geometry decision '+label)
        if not rep:assert not (target/'plan.json').exists() and not (target/'图像预览.md').exists() and not (target/'redrawn.svg').exists(),'blocked probe created false success'
        else:
            for f in report['files']:
                if f['relative_path']=='图像预览.md':verify_note(read(target/f['relative_path']),c,sem,report['decision'],f)
                elif f['relative_path']=='redrawn.svg':verify_svg(read(target/f['relative_path']),c,sem,f)
            if rep=='original_image':assert not (target/'redrawn.svg').exists(),'fallback fault probe fabricated redraw'
        exact(pins(inventory(source)),before['source_pins'],'fault probe author bytes protected');scenarios.append(dict(name=label,status='passed',purpose=before['purpose'],decision=before['decision']));record()
    for label,value in [('layout-newline','Feed\nsource'),('layout-tab','Feed\tsource'),('layout-double-space','Feed  source'),('layout-overlong','W'*40)]:edge(label,lambda s,value=value:s['elements'][0].update(label=value))
    for label,color in [('color-black','#000000'),('color-red-191','#bfc0c0'),('color-green-191','#c0bfc0'),('color-blue-191','#c0c0bf'),('color-channel-192','#c0c0c0')]:
        def source_color(s,color=color):s['legend'][0]['color']=color;s['elements'][0]['legend_id']='positive'
        edge(label,source_color)
    edge('unknown-legend-meaning',lambda s:s['legend'][0].update(meaning=None))
    edge('arrow-inside-source',lambda s:s['relations'][0].update(start=[250,210]))
    edge('arrow-enters-source',lambda s:s['relations'][0].update(start=[40,210]))
    edge('arrow-circle-off-border',lambda s:s['relations'][0].update(end=[400,210]))
    def head_occlusion(s):
        s['relations'][0].update(label='',effect='neutral',unit=None);s['elements'].append(dict(id='occluder',kind='rect',bbox=[360,177,28,30],label='X',unit=None,legend_id=None))
    edge('third-shape-arrowhead-occlusion',head_occlusion)
    edge('third-shape-occlusion',lambda s:s['elements'].append(dict(id='occluder',kind='rect',bbox=[300,190,40,40],label='X',unit=None,legend_id=None)))
    def incomplete(s):s['complete']=False;s['elements'][0]['label']=None;s['relations'][0]['effect']='unknown'
    edge('multiple-unknown-source-target',incomplete,lambda c:c.update(readability='unreadable'),lambda p:p.update(local_images=False))
    edge('unknown-target-no-svg',lambda s:None,profile_change=lambda p:p.update(verified=False,svg_supported=False))
    def circle_corner(s):
        s.update(elements=[dict(id='outer',kind='circle',bbox=[300,100,200,200],label='Circle',unit=None,legend_id=None),dict(id='inner',kind='rect',bbox=[300,100,40,40],label='X',unit=None,legend_id=None)],relations=[],legend=[],spatial_relations=[dict(id='inside',a='inner',b='outer',relation='inside')])
    edge('circle-box-false-containment',circle_corner)
    edge('observation-original',lambda s:None,lambda c:c.update(source_kind='observation'))
    edge('curve-data-still-original',lambda s:s.update(data_available=True),lambda c:c.update(source_kind='experimental_curve'))
    def guard(label,change):
        folder=root/'guards'/label;folder.mkdir();source=folder/'case';shutil.copytree(native,source);change(source);target=folder/'vault';command(label,['knowledge','image',source,profile,target],{2,3},(source,profile));assert not target.exists(),'invalid input created output '+label;scenarios.append(dict(name=label,status='rejected_before_output'));record()
    def edit_manifest(source,operation):
        c=strict_json(read(source/'case.json'));operation(c);save(source/'case.json',c)
    guard('source-sha-mismatch',lambda s:(s/'source.png').write_bytes(read(s/'source.png')+b'extra'))
    guard('source-dimension-mismatch',lambda s:edit_manifest(s,lambda c:c.update(dimensions=[959,520])))
    guard('input-traversal',lambda s:edit_manifest(s,lambda c:c['image'].update(file='../external-sentinel.txt')))
    guard('input-absolute',lambda s:edit_manifest(s,lambda c:c['image'].update(file=str(sentinel))))
    guard('input-selected-collision',lambda s:edit_manifest(s,lambda c:c['semantics'].update(file=c['image']['file'],sha256=c['image']['sha256'])))
    guard('schema-unknown-field',lambda s:edit_manifest(s,lambda c:c.update(automatic_recognition=True)))
    def invalid_geometry(s):
        sem=strict_json(read(s/'semantics.json'));sem['elements'][0]['bbox']=[900,150,220,120];save(s/'semantics.json',sem);edit_manifest(s,lambda c:c['semantics'].update(sha256=fingerprint(read(s/'semantics.json'))['sha256']))
    guard('geometry-outside-image',invalid_geometry)
    guard('extra-unselected-source',lambda s:(s/'unselected.txt').write_bytes(b'Unselected private content'))
    def corrupt_png(s):
        p=s/'source.png';raw=bytearray(read(p));raw[50]^=0xff;p.write_bytes(raw);edit_manifest(s,lambda c:c['image'].update(sha256=fingerprint(raw)['sha256']))
    guard('corrupt-png-new-pin',corrupt_png)
    def missing_nullable(s):
        sem=strict_json(read(s/'semantics.json'));sem['elements'][0].pop('unit');save(s/'semantics.json',sem);edit_manifest(s,lambda c:c['semantics'].update(sha256=fingerprint(read(s/'semantics.json'))['sha256']))
    guard('missing-required-nullable',missing_nullable)
    occupied=root/'guards'/'occupied-vault';occupied.mkdir();(occupied/'manual.md').write_bytes(b'User-owned manual note.\n');command('occupied-vault',['knowledge','image',native,profile,occupied],{2,3},(native,profile,occupied));scenarios.append(dict(name='occupied-vault',status='rejected_unchanged'));record()
    command('existing-file-output',['knowledge','image',native,profile,sentinel],{2,3},(native,profile,sentinel));scenarios.append(dict(name='existing-file-output',status='rejected_unchanged'));record()
    command('input-output-collision',['knowledge','image',native,profile,native],{2,3},(native,profile));scenarios.append(dict(name='input-output-collision',status='rejected_unchanged'));record()
    if sys.platform!='win32':
        real=root/'guards'/'real-parent';real.mkdir();link=root/'guards'/'linked-parent';link.symlink_to(real,target_is_directory=True);target=link/'vault';command('output-ancestor-symlink',['knowledge','image',native,profile,target],{2,3},(native,profile,real));assert not target.exists();scenarios.append(dict(name='output-ancestor-symlink',status='rejected_before_output'));record()
        linked=root/'guards'/'linked-case';linked.symlink_to(native,target_is_directory=True);target=root/'guards'/'linked-input-vault';command('input-ancestor-symlink',['knowledge','image',linked,profile,target],{2,3},(native,profile));assert not target.exists();scenarios.append(dict(name='input-ancestor-symlink',status='rejected_before_output'));record()
        for label,mode in [('image-file-symlink','symlink'),('image-file-hardlink','hardlink')]:
            folder=root/'guards'/label;folder.mkdir();source=folder/'case';shutil.copytree(native,source);path=source/'source.png';external=folder/'external.png';external.write_bytes(read(path));path.unlink()
            if mode=='symlink':path.symlink_to(external)
            else:os.link(external,path)
            target=folder/'vault';command(label,['knowledge','image',source,profile,target],{2,3},(external,profile));assert not target.exists();scenarios.append(dict(name=label,status='rejected_before_output'));record()
    else:scenarios.append(dict(name='Unix-link-matrix',status='not_run',reason='Windows native reparse and protection checks separately recorded'))
    exact(read(sentinel),b'User-owned image audit sentinel.\r\n','final sentinel');exact(baseline['cli_fingerprint'],fingerprint(read(cli,256*1024*1024)),'stable ordinary CLI');verify_fixture(frozen);record('passed')
    return dict(status='passed',original_cases=len(authors['cases']),cli_commands=len(commands),scenarios=len(scenarios),source_manifest=FROZEN_SHA,output=str(root),scientific_quality='not_run')


def main():
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='command',required=True);sub.add_parser('fixture')
    check=sub.add_parser('check');check.add_argument('case',type=Path);check.add_argument('profile',type=Path);check.add_argument('vault',type=Path);check.add_argument('expected',type=Path);check.add_argument('--target-profile',type=Path,help='explicit selected target original; positional profile remains frozen author baseline')
    audit=sub.add_parser('audit');audit.add_argument('cli',type=Path);audit.add_argument('output',type=Path);args=parser.parse_args()
    if args.command=='fixture':result=dict(status='passed',cases=len(verify_fixture()['cases']))
    elif args.command=='check':result=verify_output(args.case,args.profile,args.vault,args.expected,target_profile=args.target_profile)
    else:result=run_audit(args.cli,args.output)
    print(json.dumps(result,ensure_ascii=False))


if __name__=='__main__':main()
