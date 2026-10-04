#!/usr/bin/env python3
"""Run actual original NIF generation, conversion, export and graph queries. No demonstration triples."""
import argparse, hashlib, json, pathlib, shutil, struct, subprocess, sys
BASELINE = 'b2758e522dfbf273f641e9c787e6cbbcd019495c'
REPO = pathlib.Path(__file__).resolve().parents[2]
BINARY = REPO / 'target/debug/mudcrab-kg'
COMPANION = None
COMPANION_TARGET = None
def run(args, **kw):
    return subprocess.run(args, cwd=COMPANION, check=True, timeout=900, **kw)
def kg(root, *args):
    result=run([str(BINARY),'--root',str(root),*map(str,args)],capture_output=True,text=True)
    return json.loads(result.stdout)
def material_variants(out, exported, root):
    """Transform copies of real converter output to test joins and missing fields.
    Their original manifest no longer matches, so lineage must remain proposed.
    """
    results=[]
    for variant in ('reordered','absent-extension','empty-extension','duplicate-shape'):
        dest=out/'variants'/variant;shutil.copytree(exported,dest)
        report=json.loads((dest/'inspection.json').read_text());record=report['inputs'][0]['glb']
        path=dest/'artifacts'/record['path'];raw=path.read_bytes();length=struct.unpack_from('<I',raw,12)[0]
        glb=json.loads(raw[20:20+length]);materials=glb['materials']
        if variant=='reordered':
            materials.reverse()
            for mesh in glb.get('meshes',[]):
                for primitive in mesh['primitives']:
                    if 'material' in primitive:primitive['material']=len(materials)-1-primitive['material']
        elif variant=='duplicate-shape':
            materials[1]['extras']['openSkyrim']['shapeBlock']=materials[0]['extras']['openSkyrim']['shapeBlock']
        else:
            for material in materials:
                if variant=='empty-extension':material.setdefault('extensions',{})['OPEN_SKYRIM_material']={}
                else:material.get('extensions',{}).pop('OPEN_SKYRIM_material',None)
        body=json.dumps(glb,separators=(',',':')).encode();body+=b' '*((-len(body))%4)
        tail=raw[20+length:];published=struct.pack('<III',0x46546c67,2,20+len(body)+len(tail))+struct.pack('<II',len(body),0x4e4f534a)+body+tail
        changed=published!=raw
        path.write_bytes(published);record['byte_size']=len(published);record['sha256']=hashlib.sha256(published).hexdigest()
        (dest/'inspection.json').write_text(json.dumps(report))
        if variant=='duplicate-shape':
            failed=subprocess.run([str(BINARY),'--root',str(root),'ingest','materials','--dir',str(dest),'--source','synthetic-materials'],capture_output=True,text=True,timeout=60)
            assert failed.returncode!=0 and 'duplicate output shape identity' in failed.stderr
            results.append({'variant':variant,'rejected':True});continue
        imported=kg(root,'ingest','materials','--dir',dest,'--source','synthetic-materials')
        import sqlite3
        with sqlite3.connect(root/'graph.sqlite') as db:
            records=[json.loads(row[0]) for row in db.execute('SELECT json FROM records WHERE snapshot_id=?',(imported['snapshot_id'],))]
        nodes={n['id']:n for n in records if 'kind' in n}
        outputs={n['id']:n for n in nodes.values() if n['kind']=='Material' and n['attributes'].get('stage')=='published'}
        joins=[a for a in records if a.get('predicate')=='DERIVED_FROM' and a['subject'] in outputs]
        assert len(joins)==2
        for edge in joins:
            assert outputs[edge['subject']]['attributes']['shape_block']==nodes[edge['object']]['attributes']['shape_block']
            expected=('manual','proposed') if changed else ('extracted','admitted')
            assert (edge['provenance_class'],edge['initial_state'])==expected
        for n in outputs.values():
            if variant=='absent-extension':
                assert 'absent' in n['attributes']['OPEN_SKYRIM_material']
                assert 'OPEN_SKYRIM_material' not in n['attributes']['fields'].get('extensions',{})
            if variant=='empty-extension':assert n['attributes']['fields']['extensions']['OPEN_SKYRIM_material']=={}
        results.append({'variant':variant,'snapshot_id':imported['snapshot_id'],'shape_joins':len(joins),'lineage':'manual/proposed; manifest hash unmatched' if changed else 'extracted/admitted; unchanged GLB matches manifest'})
    (out/'material-variant-results.json').write_text(json.dumps(results,indent=2))
    return results
def main():
    global COMPANION, COMPANION_TARGET, BINARY
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=pathlib.Path,required=True)
    parser.add_argument('--mudcrab-repo',type=pathlib.Path,required=True,help='Companion checkout with the supplied integration patch applied')
    parser.add_argument('--companion-target',type=pathlib.Path,help='Companion target directory; defaults to CHECKOUT/target')
    parser.add_argument('--binary',type=pathlib.Path,default=BINARY)
    a=parser.parse_args()
    a.out=a.out.resolve();COMPANION=a.mudcrab_repo.resolve();COMPANION_TARGET=(a.companion_target or COMPANION/'target').resolve();BINARY=a.binary.resolve()
    if a.out.exists(): parser.error('destination must be new')
    a.out.mkdir(parents=True);data=a.out/'Data';converted=a.out/'converted';exported=a.out/'material-export';root=a.out/'graph'
    run([str(COMPANION_TARGET/'debug/examples/kg_material_fixture'),str(data)])
    run([str(COMPANION_TARGET/'debug/converter'),str(data),str(converted),'--cpu-jobs','2','--texture-encoder','cpu','--report-json',str(a.out/'pipeline-report.json')])
    selection=a.out/'selection.json';selection.write_text(json.dumps({'schema_version':1,'inputs':[{'nif':'meshes/generated.nif','glb':'meshes/generated.glb'}],'manifest':'conversion-manifest.json'}))
    before=(data/'meshes/generated.nif').read_bytes();run([str(COMPANION_TARGET/'debug/kg-export'),'--input-root',str(data),'--converted-root',str(converted),'--selection',str(selection),'--out',str(exported),'--converter-revision',BASELINE,'--synthetic'])
    assert (data/'meshes/generated.nif').read_bytes()==before
    kg(root,'init');source=kg(root,'ingest','source','--repo',COMPANION,'--source','mudcrab','--revision',BASELINE,'--path','crates/converter/src/material.rs','--path','crates/engine/src/render.rs','--path','crates/engine/src/shaders','--path','Cargo.toml','--path','Cargo.lock')
    materials=kg(root,'ingest','materials','--dir',exported,'--source','synthetic-materials','--source-snapshot',source['snapshot_id'])
    context=a.out/'context.json';context.write_text(json.dumps({'schema_version':1,'name':'materials','snapshot_ids':[materials['snapshot_id']],'policy':'public'}));ctx=kg(root,'context','create','--name','materials','--manifest',context)
    import sqlite3
    # Authored source concept selectors remain manual/proposed.
    q01=kg(root,'--context','materials','search','--query','ValidatedNifMaterial','--kind','CodeSymbol','--limit','2','--json')
    q02=kg(root,'--context','materials','trace','concept','--concept','skyrim.material.double_sided','--include-hypotheses','--limit','100','--json')
    q03=kg(root,'--context','materials','search','--query','published slot','--kind','TextureBinding','--limit','20','--json')
    for name,value in [('q01',q01),('q02',q02),('q03',q03)]: (a.out/f'{name}.json').write_text(json.dumps(value,ensure_ascii=False))
    assert q01['result']['nodes'];assert any(n['attributes'].get('stage')=='raw' for n in q02['result']['nodes'])
    assert any(n['attributes'].get('stage')=='published' for n in q02['result']['nodes'])
    assert any(x['stage']=='native' for x in q02['result']['missing_stages'])
    assert any(x['stage']=='capture' for x in q02['result']['missing_stages'])
    assert all(x['current_state']=='proposed' for x in q02['result']['assertions'] if x['assertion']['provenance_class'] in ('manual','hypothesis'))
    with sqlite3.connect(root/'graph.sqlite') as db:
        assert db.execute("SELECT count(*) FROM assertions WHERE predicate='PRUNED_REFERENCE'").fetchone()[0]>0
        output=[json.loads(row[0]) for row in db.execute("SELECT json FROM records WHERE category='node'")]
        uris=[n['attributes'].get('uri') for n in output if n['kind']=='TextureBinding' and n['attributes'].get('stage')=='published']
        assert any(u and '.opensky-srgb.ktx2' in u for u in uris)
        assert any(u and '.opensky-srgb.ktx2' not in u for u in uris)
    asset=next(n for n in output if n['kind']=='Asset' and n['attributes'].get('stage')=='original')
    for shape in (1,4):
        trace=kg(root,'--context','materials','trace','material','--asset',asset['id'],'--shape',shape,'--include-hypotheses','--limit','100')
        assert all(n['attributes'].get('shape_block') in (None,shape) for n in trace['result']['nodes'])
        assert any(n['attributes'].get('shape_block')==shape for n in trace['result']['nodes'])
        (a.out/f'shape-{shape}.json').write_text(json.dumps(trace))
    variants=material_variants(a.out,exported,root)
    report={'material_variants':variants,'baseline':BASELINE,'context_id':ctx['context_id'],'source':source,'materials':materials,'source_citations':q01['evidence'],'trace_nodes':len(q02['result']['nodes']),'trace_assertions':len(q02['result']['assertions']),'synthetic':True,'native':'not_indexed','capture':'not_indexed'}
    (a.out/'result.json').write_text(json.dumps(report,indent=2));print(json.dumps(report))
if __name__=='__main__': main()
