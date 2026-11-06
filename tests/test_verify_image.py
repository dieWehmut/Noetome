"""Frozen independent author truth and ordinary CLI image tampering regressions."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
SPEC=importlib.util.spec_from_file_location('verify_image',ROOT/'scripts/verify_image.py');V=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(V)
FIXTURE=ROOT/'fixtures/image-fidelity'


class ImageAuthorTests(unittest.TestCase):
    def source(self,name='signed_arrows'):return V.selected_source(FIXTURE/name)
    def profile(self,name='native'):return V.strict_json(V.read(FIXTURE/('profile-'+name+'.json')))

    def test_twelve_authors_are_frozen_before_compiler_outputs(self):
        self.assertEqual(len(V.verify_fixture()['cases']),12)

    def test_rewritten_expected_and_manifest_cannot_replace_frozen_truth(self):
        with tempfile.TemporaryDirectory() as directory:
            target=Path(directory)/'authors';shutil.copytree(FIXTURE,target);p=target/'expected-signed_arrows.json';value=json.loads(p.read_bytes());value['semantics']['relations'][0]['effect']='negative';V.save(p,value)
            manifest=json.loads((target/'frozen-sha256.json').read_bytes());manifest['files'][p.name]=V.fingerprint(p.read_bytes());V.save(target/'frozen-sha256.json',manifest)
            with self.assertRaises(AssertionError):V.verify_fixture(target)

    def test_decoded_whole_png_dimensions_and_real_original_bytes(self):
        c,s,source=self.source();self.assertEqual(V.png_info(source['original.png']),tuple(c['dimensions']));self.assertEqual(set(source),{'source-case.json','source-semantics.json','original.png'})
        for raw in (source['original.png'][:50],source['original.png']+b'outside'):
            with self.subTest(length=len(raw)),self.assertRaises(AssertionError):V.png_info(raw)

    def test_known_unsupported_and_no_data_curves_keep_original(self):
        for name,reason in [('unsupported_geometry','IMAGE_GEOMETRY_UNSUPPORTED'),('curve_no_data','IMAGE_CURVE_NO_DATA')]:
            c,s,_=self.source(name);self.assertEqual(V.classify(c,s,self.profile()),('original_image',[],[reason]))

    def test_unknown_source_semantics_block_clear_image_fallback(self):
        for name,code in [('unknown_label','IMAGE_UNKNOWN_LABEL'),('unknown_relation','IMAGE_UNKNOWN_RELATION'),('unknown_spatial','IMAGE_UNKNOWN_SPATIAL'),('source_unreadable','IMAGE_SOURCE_UNREADABLE')]:
            c,s,_=self.source(name)
            for profile in ('native','no-svg','unverified'):
                with self.subTest(name=name,profile=profile):self.assertEqual(V.classify(c,s,self.profile(profile)),(None,[code],[]))

    def test_local_images_are_required_and_unknown_target_cannot_redraw(self):
        c,s,_=self.source();self.assertEqual(V.classify(c,s,self.profile('no-images')),(None,['IMAGE_LOCAL_IMAGES_UNAVAILABLE'],[]))
        for field,value in [('verified',False),('app','UnknownApp'),('app_version','99.0')]:
            p=self.profile();p[field]=value;self.assertEqual(V.classify(c,s,p),('original_image',[],['IMAGE_TARGET_UNVERIFIED']))

    def test_measured_current_target_still_requires_flags_and_complete_source_semantics(self):
        c,s,_=self.source();p=self.profile();p['app_version']='1.15.0';self.assertEqual(V.classify(c,s,p),('svg_redraw',[],[]))
        for edits,reasons in [({'verified':False},['IMAGE_TARGET_UNVERIFIED']),({'svg_supported':False},['IMAGE_SVG_UNAVAILABLE']),({'verified':False,'svg_supported':False},['IMAGE_TARGET_UNVERIFIED','IMAGE_SVG_UNAVAILABLE'])]:
            self.assertEqual(V.classify(c,s,dict(p,**edits)),('original_image',[],reasons))
        unknown=copy.deepcopy(s);unknown['relations'][0]['effect']='unknown';self.assertEqual(V.classify(c,unknown,p),(None,['IMAGE_UNKNOWN_RELATION'],[]))
        curve,semantics,_=self.source('curve_no_data');self.assertEqual(V.classify(curve,semantics,p),('original_image',[],['IMAGE_CURVE_NO_DATA']))

    def test_spatial_relations_derive_from_actual_original_boxes(self):
        c,s,_=self.source('spatial');elements={e['id']:e for e in s['elements']};self.assertTrue(all(V.spatial_holds(r,elements) for r in s['spatial_relations']))
        changed=copy.deepcopy(s);changed['elements'][1]['bbox']=[100,160,140,140];self.assertEqual(V.classify(c,changed,self.profile())[:2],(None,['IMAGE_GEOMETRY_MISMATCH']))

    def test_actual_circle_containment_and_overlap_cannot_use_box_truth(self):
        circle=dict(id='outer',kind='circle',bbox=[300,100,200,200]);corner=dict(id='inner',kind='rect',bbox=[300,100,40,40]);elements={'outer':circle,'inner':corner}
        self.assertFalse(V.spatial_holds(dict(a='inner',b='outer',relation='inside'),elements))
        corner['bbox']=[360,160,40,40];self.assertTrue(V.spatial_holds(dict(a='inner',b='outer',relation='inside'),elements))
        corner.update(kind='circle',bbox=[470,270,40,40]);self.assertFalse(V.spatial_holds(dict(a='inner',b='outer',relation='overlaps'),elements))
        corner.update(kind='rect',bbox=[480,280,20,20]);self.assertFalse(V.spatial_holds(dict(a='inner',b='outer',relation='overlaps'),elements))

    def test_relation_points_must_touch_boundary_and_leave_source(self):
        c,s,_=self.source();elements={e['id']:e for e in s['elements']};r=copy.deepcopy(s['relations'][0]);self.assertTrue(V.relation_geometry(r,elements))
        r['start']=[250,210];self.assertFalse(V.relation_geometry(r,elements))
        r=copy.deepcopy(s['relations'][0]);r['end']=[400,210];self.assertFalse(V.relation_geometry(r,elements))
        r=copy.deepcopy(s['relations'][0]);r['start']=[40,210];self.assertFalse(V.relation_geometry(r,elements))

    def test_multiple_unknowns_and_target_limits_are_not_silently_collapsed(self):
        c,s,_=self.source();s['complete']=False;s['elements'][0]['label']=None;s['relations'][0]['effect']='unknown';p=self.profile('no-images');c['readability']='unreadable'
        self.assertEqual(V.classify(c,s,p),(None,['IMAGE_DECLARATION_INCOMPLETE','IMAGE_UNKNOWN_LABEL','IMAGE_UNKNOWN_RELATION','IMAGE_SOURCE_UNREADABLE','IMAGE_LOCAL_IMAGES_UNAVAILABLE'],[]))
        c,s,_=self.source();p=self.profile('no-svg');p['verified']=False;self.assertEqual(V.classify(c,s,p)[2],['IMAGE_TARGET_UNVERIFIED','IMAGE_SVG_UNAVAILABLE'])
        s['elements']=[];s['relations']=[];s['spatial_relations']=[];s['legend']=[];V.semantics_schema(s,c['dimensions']);self.assertEqual(V.classify(c,s,self.profile())[:2],(None,['IMAGE_UNKNOWN_LABEL']))

    def test_direction_and_effect_are_independently_preserved(self):
        c,s,_=self.source();base=V.svg_primitives(c,s);changed=copy.deepcopy(s);changed['relations'][0]['direction']='reverse';reversed=V.svg_primitives(c,changed)
        self.assertNotEqual(next(p['attrs']['points'] for p in base if p['attrs']['id']=='relation-gain-head'),next(p['attrs']['points'] for p in reversed if p['attrs']['id']=='relation-gain-head'))
        changed=copy.deepcopy(s);changed['relations'][0]['effect']='negative';flipped=V.svg_primitives(c,changed)
        self.assertNotEqual(next(p['value'] for p in base if p['attrs']['id']=='relation-gain-effect'),next(p['value'] for p in flipped if p['attrs']['id']=='relation-gain-effect'))

    def test_literal_xml_metacharacters_stay_source_text(self):
        c,s,_=self.source('escaped_labels');self.assertIn('<script>',s['caption']);self.assertEqual(s['elements'][0]['label'],'Dose <low> & "safe"')
        _,nodes=V.svg_nodes(b'<svg xmlns="http://www.w3.org/2000/svg"><text id="a">A &lt; B &amp; C</text></svg>');self.assertEqual(nodes[0]['value'],'A < B & C')

    def test_svg_external_entities_hidden_groups_and_extra_children_rejected(self):
        for raw in (b'<!DOCTYPE svg><svg xmlns="http://www.w3.org/2000/svg"/>',b'<svg xmlns="http://www.w3.org/2000/svg"><g><text>A</text></g></svg>',b'<svg xmlns="http://www.w3.org/2000/svg"><script>1</script></svg>'):
            with self.subTest(raw=raw),self.assertRaises(AssertionError):V.svg_nodes(raw)

    def test_markdown_fences_preserve_source_markup_and_field_ownership(self):
        nodes=V.markdown_nodes(b'# title\n\n## field\n\n````text\n```\n## hidden\n![[original.png]]\n````\n')
        self.assertEqual([n['kind'] for n in nodes],['heading','heading','literal']);self.assertEqual(nodes[-1]['value'],'```\n## hidden\n![[original.png]]')
        c,s,_=self.source();wanted=V.note_semantic_nodes(c,s,dict(representation='svg_redraw',fallback_reasons=[]));index=next(i for i,n in enumerate(wanted) if n.get('mapping')==('element_unit','semantics.elements[0].unit'));self.assertEqual(wanted[index-1],dict(kind='label',value='单位：'))

    def test_unsupported_text_and_actual_geometry_overlap_keep_source(self):
        c,s,_=self.source()
        for label in ('Feed\nsource','Feed\tsource','Feed  source','W'*40):
            changed=copy.deepcopy(s);changed['elements'][0]['label']=label
            with self.subTest(label=label):self.assertEqual(V.classify(c,changed,self.profile()),('original_image',[],['IMAGE_TEXT_LAYOUT']))
        c,s,_=self.source('spatial');s['elements'][1]['bbox']=[540,160,140,140]
        self.assertEqual(V.classify(c,s,self.profile()),('original_image',[],['IMAGE_GEOMETRY_LAYOUT']))
        c,s,_=self.source();s['elements'].append(dict(id='occluder',kind='rect',bbox=[300,190,40,40],label='X',unit=None,legend_id=None))
        self.assertEqual(V.classify(c,s,self.profile()),('original_image',[],['IMAGE_GEOMETRY_LAYOUT']))

    def test_arrowhead_outside_centerline_cannot_occlude_third_shape(self):
        c,s,_=self.source();s['relations'][0].update(label='',effect='neutral',unit=None);third=dict(id='occluder',kind='rect',bbox=[360,177,28,30],label='X',unit=None,legend_id=None);s['elements'].append(third)
        self.assertTrue(V.triangle_hits_element([(390,210),(380,215),(380,205)],third));self.assertEqual(V.classify(c,s,self.profile()),('original_image',[],['IMAGE_GEOMETRY_LAYOUT']))
        self.assertFalse(V.triangle_hits_element([(390,210),(380,215),(380,205)],dict(kind='rect',bbox=[300,100,30,30])))

    def test_known_dark_colors_keep_original_without_reclassifying_labels(self):
        c,s,_=self.source();p=self.profile()
        for color in ('#000000','#bfc0c0','#c0bfc0','#c0c0bf'):
            changed=copy.deepcopy(s);changed['legend'][0]['color']=color;changed['elements'][0]['legend_id']='positive';V.semantics_schema(changed,c['dimensions'])
            with self.subTest(color=color):
                self.assertEqual(V.classify(c,changed,p),('original_image',[],['IMAGE_COLOR_CONTRAST']));self.assertEqual(changed['legend'][0]['color'],color);self.assertEqual(changed['elements'][0]['label'],'Feed')
        changed=copy.deepcopy(s);changed['legend'][0]['color']='#c0c0c0';changed['elements'][0]['legend_id']='positive';self.assertEqual(V.classify(c,changed,p),('svg_redraw',[],[]))
        changed['legend'][0]['color']='#000000';changed['elements'][0]['label']=None;self.assertEqual(V.classify(c,changed,p),(None,['IMAGE_UNKNOWN_LABEL'],[]))
        c['source_kind']='experimental_curve';changed['elements'][0]['label']='Feed';self.assertEqual(V.classify(c,changed,p),('original_image',[],['IMAGE_CURVE_NO_DATA']))

    def test_cross_namespace_ids_and_unicode_controls_follow_manual_contract(self):
        c,s,_=self.source();s['legend'][0]['id']='feed';s['relations'][0]['legend_id']='feed';V.semantics_schema(s,c['dimensions'])
        for control in ('\x00','\x7f','\x85'):
            with self.subTest(control=control),self.assertRaises(AssertionError):V.string('label'+control)
        V.string('line\r\nsource')

    def test_strict_nullable_version_coordinates_and_ancestor_admission(self):
        with tempfile.TemporaryDirectory() as directory:
            base=Path(directory)
            for name,edit in [('boolean-version',lambda c:c.update(schema_version=True)),('bad-size',lambda c:c.update(dimensions=[True,520])),('traversal',lambda c:c['image'].update(file='../source.png')),('collision',lambda c:c['image'].update(file='case.json'))]:
                target=base/name;shutil.copytree(FIXTURE/'signed_arrows',target);c=json.loads((target/'case.json').read_bytes());edit(c);V.save(target/'case.json',c)
                with self.subTest(name=name),self.assertRaises((AssertionError,KeyError)):V.selected_source(target)
            if os.name!='nt':
                linked=base/'linked';linked.symlink_to(FIXTURE/'signed_arrows',target_is_directory=True)
                with self.assertRaises(AssertionError):V.selected_source(linked)


class ImageCliReaderTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        selected=os.environ.get('NOETOME_TEST_BINARY')
        if not selected:raise unittest.SkipTest('ordinary CLI selected by NOETOME_TEST_BINARY; separate audit exercises all cases')
        cls.cli=Path(selected);assert cls.cli.is_file(),'selected ordinary CLI exists';cls.temp=tempfile.TemporaryDirectory(prefix='noetome-image-reader-');cls.root=Path(cls.temp.name);cls.vault=cls.root/'baseline'
        result=subprocess.run([str(cls.cli),'knowledge','image',str(FIXTURE/'signed_arrows'),str(FIXTURE/'profile-native.json'),str(cls.vault)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=60);assert result.returncode==0,result.stderr.decode(errors='replace');V.verify_output(FIXTURE/'signed_arrows',FIXTURE/'profile-native.json',cls.vault,FIXTURE/'expected-signed_arrows.json')
    @classmethod
    def tearDownClass(cls):cls.temp.cleanup()
    def cloned(self):
        target=self.root/(self._testMethodName+'-'+str(len(list(self.root.iterdir()))));shutil.copytree(self.vault,target);return target
    def checked(self,vault):return V.verify_output(FIXTURE/'signed_arrows',FIXTURE/'profile-native.json',vault,FIXTURE/'expected-signed_arrows.json')

    def test_real_cli_readback_preserves_original_and_full_semantics(self):self.assertEqual(self.checked(self.vault)['representation'],'svg_redraw')

    def test_explicit_target_original_recomputes_fallback_and_preserves_author_freeze(self):
        author_before=V.pins(V.inventory(FIXTURE));profile=self.root/'image-target.json';value=V.strict_json(V.read(FIXTURE/'profile-native.json'));value['app_version']='99.0';V.save(profile,value);target=self.root/'image-explicit-target'
        result=subprocess.run([str(self.cli),'knowledge','image',str(FIXTURE/'signed_arrows'),str(profile),str(target)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=60);self.assertEqual(result.returncode,0,result.stderr.decode(errors='replace'))
        checked=V.verify_output(FIXTURE/'signed_arrows',FIXTURE/'profile-native.json',target,FIXTURE/'expected-signed_arrows.json',target_profile=profile);self.assertEqual(checked['representation'],'original_image')
        audit=subprocess.run([V.sys.executable,str(ROOT/'scripts/verify_image.py'),'check',str(FIXTURE/'signed_arrows'),str(FIXTURE/'profile-native.json'),str(target),str(FIXTURE/'expected-signed_arrows.json'),'--target-profile',str(profile)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30);self.assertEqual(audit.returncode,0,audit.stderr.decode(errors='replace'));self.assertEqual(V.strict_json(audit.stdout),checked)
        with self.assertRaises(AssertionError):self.checked(target)
        with self.assertRaisesRegex(AssertionError,'selected frozen target profile'):V.verify_output(FIXTURE/'signed_arrows',profile,target,FIXTURE/'expected-signed_arrows.json',target_profile=profile)
        self.assertEqual(V.pins(V.inventory(FIXTURE)),author_before)

    def test_explicit_target_cannot_turn_unknown_source_relation_into_fallback_success(self):
        profile=self.root/'image-blocked-target.json';value=V.strict_json(V.read(FIXTURE/'profile-native.json'));value['app_version']='99.0';V.save(profile,value);target=self.root/'image-unknown-explicit-target'
        result=subprocess.run([str(self.cli),'knowledge','image',str(FIXTURE/'unknown_relation'),str(profile),str(target)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=60);self.assertEqual(result.returncode,3)
        checked=V.verify_output(FIXTURE/'unknown_relation',FIXTURE/'profile-native.json',target,FIXTURE/'expected-unknown_relation.json',target_profile=profile);self.assertIsNone(checked['representation']);self.assertEqual(checked['issue_codes'],['IMAGE_UNKNOWN_RELATION'])

    def test_synchronized_unknown_target_svg_claim_rejected_with_explicit_target(self):
        target=self.cloned();profile=self.root/'image-forged-target.json';value=V.strict_json(V.read(FIXTURE/'profile-native.json'));value['app_version']='99.0';V.save(profile,value);(target/'profile.json').write_bytes(profile.read_bytes());report=V.strict_json(V.read(target/'image.json'));report['profile']=value;report['inputs']['profile.json']=V.fingerprint(profile.read_bytes());V.save(target/'image.json',report);V.resynchronize_result(target)
        with self.assertRaisesRegex(AssertionError,'source-grounded image outcome'):V.verify_output(FIXTURE/'signed_arrows',FIXTURE/'profile-native.json',target,FIXTURE/'expected-signed_arrows.json',target_profile=profile)

    def test_explicit_measured_target_still_rejects_synced_arrowhead_loss(self):
        target=self.cloned();V.mutate_svg(target,'relation-gain-head',lambda raw:b'');V.resynchronize_result(target)
        with self.assertRaises(AssertionError):V.verify_output(FIXTURE/'signed_arrows',FIXTURE/'profile-native.json',target,FIXTURE/'expected-signed_arrows.json',target_profile=FIXTURE/'profile-native.json')

    def test_actual_svg_unit_effect_label_and_shape_losses_rejected_after_full_resync(self):
        for id in ('image-canvas','element-feed-label','element-feed-unit','relation-gain-effect','relation-gain-head','legend-positive-meaning','element-feed'):
            target=self.cloned();V.mutate_svg(target,id,lambda raw:b'');V.resynchronize_result(target)
            with self.subTest(id=id),self.assertRaises(AssertionError):self.checked(target)

    def test_actual_arrowhead_and_coordinates_cannot_be_reversed(self):
        target=self.cloned();V.mutate_svg(target,'relation-gain-head',lambda raw:raw.replace(b'390.000',b'260.000'));V.resynchronize_result(target)
        with self.assertRaises(AssertionError):self.checked(target)
        target=self.cloned();V.mutate_svg(target,'element-feed',lambda raw:raw.replace(b'x="40"',b'x="400"'));V.resynchronize_result(target)
        with self.assertRaises(AssertionError):self.checked(target)

    def test_note_field_labels_cannot_reassign_same_flattened_values(self):
        target=self.cloned();p=target/'图像预览.md';raw=p.read_bytes();unit='单位：'.encode();legend='图例：'.encode();altered=raw.replace(unit,b'__TEMP_LABEL__',1).replace(legend,unit,1).replace(b'__TEMP_LABEL__',legend,1)
        self.assertEqual(sorted(n['value'] for n in V.markdown_nodes(raw)),sorted(n['value'] for n in V.markdown_nodes(altered)));p.write_bytes(altered)
        old=V.markdown_nodes(raw);new=V.markdown_nodes(altered);r=V.strict_json(V.read(target/'image.json'))
        for m in r['files'][0]['mappings']:
            i=next(i for i,n in enumerate(old) if n['start']==m['start_byte'] and n['end']==m['end_byte']);m.update(start_byte=new[i]['start'],end_byte=new[i]['end'])
        V.save(target/'image.json',r);V.resynchronize_result(target)
        with self.assertRaises(AssertionError):self.checked(target)

    def test_actual_dark_svg_is_rejected_after_source_plan_and_reports_resynchronize(self):
        target=self.cloned();V.mutate_svg(target,'element-feed',lambda raw:raw.replace(b'fill="#ffffff"',b'fill="#000000"'));V.mutate_svg(target,'legend-positive',lambda raw:raw.replace(b'fill="#c5efcb"',b'fill="#000000"'))
        c=V.strict_json(V.read(target/'source-case.json'));sem=V.strict_json(V.read(target/'source-semantics.json'));sem['legend'][0]['color']='#000000';sem['elements'][0]['legend_id']='positive';V.save(target/'source-semantics.json',sem);c['semantics']['sha256']=V.fingerprint(V.read(target/'source-semantics.json'))['sha256'];V.save(target/'source-case.json',c)
        plan=V.strict_json(V.read(target/'plan.json'));plan.update(case=c,semantics=sem);V.save(target/'plan.json',plan);report=V.strict_json(V.read(target/'image.json'));report.update(case=c,semantics=sem);report['inputs']['source-case.json']=V.fingerprint(V.read(target/'source-case.json'));report['inputs']['source-semantics.json']=V.fingerprint(V.read(target/'source-semantics.json'));V.save(target/'image.json',report);V.resynchronize_result(target)
        report=V.strict_json(V.read(target/'image.json'));f=next(f for f in report['files'] if f['relative_path']=='redrawn.svg');self.assertEqual(f['content_sha256'],V.fingerprint(V.read(target/'redrawn.svg'))['sha256']);self.assertEqual(report['semantics'],sem)
        with self.assertRaisesRegex(AssertionError,'IMAGE_COLOR_CONTRAST'):V.verify_svg(V.read(target/'redrawn.svg'),c,sem,f)

    def test_replaced_original_and_fabricated_data_are_refused(self):
        target=self.cloned();(target/'original.png').write_bytes(V.read(FIXTURE/'source_unreadable/source.png'));V.resynchronize_result(target)
        with self.assertRaises(AssertionError):self.checked(target)
        target=self.cloned();(target/'fake.csv').write_bytes(b'x,y\n0,0\n');V.resynchronize_result(target)
        with self.assertRaises(AssertionError):self.checked(target)


if __name__=='__main__':unittest.main()
