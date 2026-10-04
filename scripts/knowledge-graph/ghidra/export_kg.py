#!/usr/bin/env python3
"""Read an existing Ghidra 11.4/PyGhidra 2.2 analyzed program. Never analyze or save it."""
import argparse
import collections
import json
import pathlib
import sys
import time
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from kg_bundle import Builder, canonical, digest, file_hash


def address(addr): return {'space':str(addr.getAddressSpace().getName()),'va':hex(int(addr.getOffset()) & ((1<<64)-1))}
def rva(addr, base):
    if str(addr.getAddressSpace().getName()) != str(base.getAddressSpace().getName()):
        raise ValueError('address is not in module image space')
    offset=int(addr.getOffset())-int(base.getOffset())
    if offset<0: raise ValueError('address precedes image base')
    return hex(offset)
def bounded(iterable, count):
    it=iter(iterable)
    for _ in range(count):
        try: yield next(it)
        except StopIteration: return
    try: next(it)
    except StopIteration: return
    raise ValueError('declared extraction count limit exceeded')
def selection(value):
    allowed={'schema_version','source','functions','types','level','max_functions','max_relationships','timeout_seconds','neighbor_hops','max_pcode_ops','max_bytes','max_seconds','global_metadata','imports','dependencies','synthetic','analysis_revision'}
    if set(value)-allowed or value.get('schema_version')!=1: raise ValueError('invalid Ghidra selection')
    cfg={'level':0,'max_functions':200,'max_relationships':50000,'timeout_seconds':30,'neighbor_hops':2,'max_pcode_ops':10000,'max_bytes':128*1024*1024,'max_seconds':300,**value}
    for name,lo,hi in [('level',0,2),('max_functions',1,200),('max_relationships',1,50000),('timeout_seconds',1,30),('neighbor_hops',0,2),('max_pcode_ops',1,50000),('max_seconds',1,3600),('max_bytes',1024,512*1024*1024)]:
        if type(cfg[name]) is not int or not lo<=cfg[name]<=hi: raise ValueError('invalid bounded selection: '+name)
    if not cfg.get('functions') and not cfg.get('global_metadata'): raise ValueError('explicit function RVAs or global_metadata required')
    if len(cfg.get('functions',[]))>cfg['max_functions'] or len(cfg.get('types',[]))>256: raise ValueError('selection exceeds limits')
    if len(set(cfg.get('functions',[])))!=len(cfg.get('functions',[])): raise ValueError('duplicate function selection')
    return cfg

def extract(program, cfg, module_hash, version, decompiler=None, monitor=None):
    """API-facing extraction, separately testable from Java project opening and bundle storage."""
    cfg=selection(cfg);base=program.getImageBase();fm=program.getFunctionManager();rm=program.getReferenceManager();listing=program.getListing()
    before=(bool(program.isChanged()),int(program.getModificationNumber()))
    start=time.monotonic();queue=collections.deque();functions={};types={};diagnostics=[]
    def check():
        if time.monotonic()-start>cfg['max_seconds']: raise TimeoutError('native extraction request timeout')
        if monitor is not None: monitor.checkCancelled()
    if cfg.get('global_metadata'):
        for f in bounded(fm.getFunctions(True), cfg['max_functions']):queue.append((f,0))
    else:
        for offset in cfg['functions']:
            if not isinstance(offset,str) or not offset.startswith('0x') or hex(int(offset,16))!=offset: raise ValueError('canonical function RVA required')
            f=fm.getFunctionAt(base.add(int(offset,16)))
            if f is None: raise ValueError('selected function unavailable')
            queue.append((f,0))
    dtm=program.getDataTypeManager()
    def collect_type(dt):
        if dt is None:return None
        path=str(dt.getPathName())
        if path in types:return path
        if len(types)>=256: diagnostics.append(('TYPE_LIMIT','Selected type closure truncated',path));return None
        archive=dt.getSourceArchive();origin={'status':'recovered_or_analysis','archive':str(archive.getName()) if archive else None,
          'archive_id':str(archive.getSourceArchiveID()) if archive else None}
        record={'path':path,'name':str(dt.getName()),'length':int(dt.getLength()),'origin':origin,'fields':[]}
        types[path]=record
        if hasattr(dt,'getComponents'):
            for component in bounded(dt.getComponents(),1024):
                record['fields'].append({'offset':int(component.getOffset()),'ordinal':int(component.getOrdinal()),'name':str(component.getFieldName() or ''),
                 'length':int(component.getLength()),'type':str(component.getDataType().getPathName()),'comment':str(component.getComment() or '')})
        return path
    for path in cfg.get('types',[]):
        dt=dtm.getDataType(path)
        if dt is None: diagnostics.append(('UNRESOLVED_TYPE','Selected analysis type unavailable',path))
        else:collect_type(dt)
    relationships=0
    while queue:
        check();f,depth=queue.popleft();entry=f.getEntryPoint()
        # External-space entries have no module RVA. Keep them distinct from image addresses.
        external=bool(f.isExternal());key=str(entry) if external else rva(entry,base)
        if key in functions:continue
        if len(functions)>=cfg['max_functions']:
            diagnostics.append(('FUNCTION_LIMIT','Neighbor selection truncated',key));break
        params=list(f.getParameters())
        record={'rva':None if external else key,'entry':address(entry),'name':str(f.getName()),'label_origin':str(f.getSymbol().getSource()),
          'signature':str(f.getSignature()),'signature_origin':str(f.getSignatureSource()),'external':external,'thunk':bool(f.isThunk()),
          'return_type':collect_type(f.getReturnType()),'parameter_types':[collect_type(p.getDataType()) for p in params],
          'body_ranges':[],'direct_calls':[],'indirect_calls':[],'data_references':[],'decompilation':{'status':'not_run'},'pcode':[],'field_accesses':[]}
        functions[key]=record
        if external:continue
        record['body_ranges']=[{'start':rva(x.getMinAddress(),base),'end_inclusive':rva(x.getMaxAddress(),base)} for x in bounded(f.getBody().getAddressRanges(),4096)]
        for ins in bounded(listing.getInstructions(f.getBody(),True),200000):
            check();site=rva(ins.getAddress(),base);flow=ins.getFlowType()
            if flow.isCall() and flow.isComputed():record['indirect_calls'].append({'instruction_rva':site,'status':'unresolved','flow_type':str(flow)})
            for ref in rm.getReferencesFrom(ins.getAddress()):
                check();rt=ref.getReferenceType();target=fm.getFunctionAt(ref.getToAddress())
                if rt.isCall() and not flow.isComputed() and target is not None:
                    if relationships>=cfg['max_relationships']:
                        diagnostics.append(('RELATIONSHIP_LIMIT','Native relationship limit reached',key));break
                    target_key=str(target.getEntryPoint()) if target.isExternal() else rva(target.getEntryPoint(),base)
                    record['direct_calls'].append({'instruction_rva':site,'target':target_key,'reference_type':str(rt)});relationships+=1
                    if depth<cfg['neighbor_hops']:queue.append((target,depth+1))
                elif rt.isData() and len(record['data_references'])<1000:
                    data=listing.getDefinedDataAt(ref.getToAddress())
                    record['data_references'].append({'instruction_rva':site,'target':address(ref.getToAddress()),'type':str(data.getDataType()) if data else None,
                     'value':str(data.getValue())[:2048] if data and data.hasStringValue() else None})
        selected_ops=[]
        if cfg['level']>=1 and decompiler is not None:
            result=decompiler.decompileFunction(f,cfg['timeout_seconds'],monitor)
            if not result.decompileCompleted():
                record['decompilation']={'status':'timeout' if result.isTimedOut() else 'failed','message':str(result.getErrorMessage())[:1024]}
                diagnostics.append(('DECOMPILE_FAILED','Selected function decompilation failed or timed out',key))
            else:
                text=str(result.getDecompiledFunction().getC())
                record['decompilation']={'status':'complete','style':'decompile','text':text[:262144],'truncated':len(text)>262144}
                if cfg['level']==2:
                    high=result.getHighFunction();ops=high.getPcodeOps()
                    for op in ops:
                        check()
                        if len(record['pcode'])>=cfg['max_pcode_ops']:
                            diagnostics.append(('PCODE_LIMIT','Selected P-code slice truncated',key));break
                        selected_ops.append(op)
                        seq=op.getSeqnum();out=op.getOutput()
                        def varnode(v):
                            hv=v.getHigh();dt=hv.getDataType() if hv else None
                            return {'address':address(v.getAddress()),'size':int(v.getSize()),'constant':bool(v.isConstant()),'type':str(dt.getPathName()) if dt else None}
                        record['pcode'].append({'instruction_rva':rva(seq.getTarget(),base),'sequence':int(seq.getTime()),'opcode':str(op.getMnemonic()),
                          'inputs':[varnode(op.getInput(i)) for i in range(op.getNumInputs())], 'output':varnode(out) if out else None})
        if cfg['level']==2 and record['decompilation']['status']=='complete':
            # Resolve only typed struct-pointer arithmetic with a constant field offset.
            # The output's immediate def-use must be the LOAD/STORE address operand.
            selected_sites={(p['instruction_rva'],p['sequence']) for p in record['pcode']}
            for op in selected_ops:
                check()
                if str(op.getMnemonic()) not in ('PTRSUB','PTRADD') or op.getNumInputs()<2:continue
                base_node=op.getInput(0);high_var=base_node.getHigh();pointer_type=high_var.getDataType() if high_var else None
                if pointer_type is None or not hasattr(pointer_type,'getDataType') or not op.getInput(1).isConstant():continue
                owner_type=pointer_type.getDataType()
                if not hasattr(owner_type,'getComponentAt'):continue
                offset=int(op.getInput(1).getOffset())
                if str(op.getMnemonic())=='PTRADD':
                    if op.getNumInputs()!=3 or not op.getInput(2).isConstant():continue
                    offset*=int(op.getInput(2).getOffset())
                component=owner_type.getComponentAt(offset)
                if component is None or int(component.getOffset())!=offset:continue
                type_path=collect_type(owner_type);output=op.getOutput()
                if output is None or type_path is None:continue
                for use in bounded(output.getDescendants(),100):
                    mnemonic=str(use.getMnemonic())
                    if mnemonic not in ('LOAD','STORE') or use.getNumInputs()<2 or not use.getInput(1).equals(output):continue
                    seq=use.getSeqnum()
                    if (rva(seq.getTarget(),base),int(seq.getTime())) not in selected_sites:continue
                    record['field_accesses'].append({'type_path':type_path,'offset':offset,'access':'READS_FIELD' if mnemonic=='LOAD' else 'WRITES_FIELD',
                      'instruction_rva':rva(seq.getTarget(),base),'sequence':int(seq.getTime()),'basis':'recovered_type','pointer_operation':str(op),
                      'access_operation':str(use),'type_origin':types[type_path]['origin']})
                    if len(record['field_accesses'])>cfg['max_relationships']:raise ValueError('field access relationship limit')
        check()
    after=(bool(program.isChanged()),int(program.getModificationNumber()))
    if after!=before:raise RuntimeError('program state changed during read-only export')
    return {'module_sha256':module_hash,'program_id':str(program.getName()),'preferred_image_base':address(base),
      'language':str(program.getLanguageID()),'compiler_spec':str(program.getCompilerSpec().getCompilerSpecID()),'tool_version':version,
      'functions':functions,'types':types,'imports':cfg.get('imports',[]),'diagnostics':diagnostics,'mutation_check':'unchanged'}

def normalize(raw, cfg):
    fingerprint=digest(raw)
    b=Builder(cfg.get('source','native-analysis'),'binary',cfg.get('analysis_revision',fingerprint),fingerprint,'ghidra-export',
      {'adapter':'ghidra-11.4-api-v2','tool_version':raw['tool_version'],'module_sha256':raw['module_sha256'],'selection':cfg},
      {'level':cfg['level'],'functions':sorted(raw['functions']),'types':sorted(raw['types']),'analysis_fingerprint':fingerprint},cfg.get('synthetic',False),cfg.get('dependencies',[]))
    art=b.artifact('native-analysis.json',canonical(raw));module=raw['module_sha256'];base_space=raw['preferred_image_base']['space'];nodes={};type_nodes={};field_nodes={}
    for path,record in raw['types'].items():
        ev=b.evidence(art,{'kind':'json_pointer','pointer':'/types/'+path.replace('~','~0').replace('/','~1')},'Ghidra recovered data type')
        t=b.node('DataType',path,record['name'],record,[ev]);type_nodes[path]=t
        for field in record['fields']:
            f=b.node('TypeField',path+':'+str(field['offset'])+':'+str(field['ordinal']),field['name'] or 'unnamed field',{'stage':'native','origin':'recovered_type',**field},[ev]);field_nodes[(path,field['offset'])]=f
            b.edge(t,'HAS_FIELD',f,{'origin':'analysis type, not debug-symbol truth'},[ev])
    for key,f in raw['functions'].items():
        locator={'kind':'json_pointer','pointer':'/functions/'+key.replace('~','~0').replace('/','~1')} if f['external'] else {'kind':'binary_address','module_sha256':module,'space':base_space,'rva':key}
        ev=b.evidence(art,locator,'Ghidra function metadata and selected analysis')
        logical=module+':'+f['entry']['space']+':'+(f['rva'] or f['entry']['va'])
        attrs={**f,'analysis_fingerprint':fingerprint,'module_sha256':module,'stage':'native','evidence_independence':'imported names/types retain community lineage'}
        n=b.node('ExternalSymbol' if f['external'] else 'BinaryFunction',logical,f['name'],attrs,[ev]);nodes[key]=(n,ev)
        for path in set([f['return_type'],*f['parameter_types']])-{None}:
            if path in type_nodes:b.edge(n,'HAS_TYPE',type_nodes[path],{'origin':'analysis signature'},[ev])
        for op in f['pcode']:
            pe=b.evidence(art,{'kind':'pcode','module_sha256':module,'space':base_space,'function_rva':key,'instruction_rva':op['instruction_rva'],'sequence':op['sequence'],'style':'decompile'},'Ghidra high P-code; typed def-use context')
            # Keep the slice in the immutable artifact. No text/offset matching is admitted as field access.
    for key,f in raw['functions'].items():
        n,ev=nodes[key]
        for call in f['direct_calls']:
            if call['target'] not in nodes:
                b.diagnostic('UNRESOLVED_TARGET','Direct target outside selected function closure',{'function_rva':key,'instruction_rva':call['instruction_rva']},'native_calls');continue
            ce=b.evidence(art,{'kind':'binary_address','module_sha256':module,'space':base_space,'rva':key,'instruction_rva':call['instruction_rva']},'Ghidra direct call reference')
            b.edge(n,'CALLS_DIRECT',nodes[call['target']][0],call,[ce])
    for key,f in raw['functions'].items():
        for access in f.get('field_accesses',[]):
            field=field_nodes.get((access['type_path'],access['offset']))
            if field is None:continue
            ev=b.evidence(art,{'kind':'pcode','module_sha256':module,'space':base_space,'function_rva':key,'instruction_rva':access['instruction_rva'],'sequence':access['sequence'],'style':'decompile'},'typed base + constant offset + immediate LOAD/STORE def-use')
            b.edge(nodes[key][0],access['access'],field,access,[ev])
    for imported in raw['imports']:
        if not isinstance(imported,dict) or not all(k in imported for k in ('origin','revision','kind','selector')): raise ValueError('imports require origin, revision, kind and exact selector')
        ev=b.evidence(art,{'kind':'json_pointer','pointer':'/imports/'+str(raw['imports'].index(imported))},'operator-declared community import lineage','manual')
        doc=b.node('DocumentSection',digest(imported),'analysis import '+imported['kind'],{'lineage':imported,'independent_original_evidence':False},[ev])
        targets=nodes if imported['kind'] in ('name','signature') else {k:(v,ev) for k,v in type_nodes.items()}
        if imported['selector'] in targets:
            b.edge(doc,'NAMES',targets[imported['selector']][0],{'lineage_status':'operator_declared','independent_corroboration':False},[ev],'manual')
            if imported.get('source_node_id') and imported.get('evidence_ids'):
                b.edge(imported['source_node_id'],'NAMES',targets[imported['selector']][0],{'lineage':imported,'lineage_status':'operator_declared_exact_selector','independent_corroboration':False},[ev,*imported['evidence_ids']],'manual')
    for code,msg,sel in raw['diagnostics']:b.diagnostic(code,msg,{'selector':sel},'native_analysis')
    failures=sum(f['decompilation']['status'] in ('failed','timeout') for f in raw['functions'].values())
    b.capability('native_metadata',len(nodes),skipped=None if any(x[0].endswith('LIMIT') for x in raw['diagnostics']) else 0,detail='Selected existing analysis only; labels and signatures retain origin; skipped neighbor count unknown after limits',status='partial' if any(x[0].endswith('LIMIT') for x in raw['diagnostics']) else 'complete')
    b.capability('native_decompilation',len(nodes) if cfg['level'] else 0,failures,detail='Selected decompiler artifacts; no program mutation',status='not_run' if not cfg['level'] else None)
    b.capability('native_field_access',sum(len(f.get('field_accesses',[])) for f in raw['functions'].values()),detail='Only recovered struct-pointer + constant offset + immediate LOAD/STORE; other interpretations unresolved',status='complete' if cfg['level']==2 else 'not_run')
    return b

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--project-location',type=pathlib.Path,required=True);p.add_argument('--project-name',required=True);p.add_argument('--program',required=True);p.add_argument('--module',type=pathlib.Path,required=True);p.add_argument('--selection',type=pathlib.Path,required=True);p.add_argument('--out',type=pathlib.Path,required=True);a=p.parse_args()
    cfg=selection(json.loads(a.selection.read_text()));module_hash=file_hash(a.module)
    import pyghidra
    pyghidra.start()
    from ghidra.framework.project import DefaultProjectManager
    from ghidra.framework.model import ProjectLocator
    from ghidra.util.task import TaskMonitor
    from ghidra.framework import Application
    from ghidra.app.decompiler import DecompInterface
    from java.lang import Object
    project=DefaultProjectManager().openProject(ProjectLocator(str(a.project_location),a.project_name),False,False)
    consumer=Object();program=None;dec=None
    try:
        domain=project.getProjectData().getFile(a.program)
        if domain is None:raise ValueError('existing analyzed program unavailable; no import is performed')
        program=domain.getReadOnlyDomainObject(consumer,-1,TaskMonitor.DUMMY)
        if str(program.getExecutableSHA256()).lower()!=module_hash:raise ValueError('module hash does not match analyzed program')
        if cfg['level']:
            dec=DecompInterface();dec.setSimplificationStyle('decompile')
            if not dec.openProgram(program):raise RuntimeError('decompiler could not open program')
        raw=extract(program,cfg,module_hash,{'ghidra':str(Application.getApplicationVersion()),'pyghidra':pyghidra.__version__},dec,TaskMonitor.DUMMY)
        result=normalize(raw,cfg).write(a.out,cfg['max_bytes']);print(json.dumps(result))
    finally:
        if dec is not None:dec.dispose()
        if program is not None:program.release(consumer)
        project.close()
if __name__=='__main__':
    from kg_bundle import supervised
    supervised(__file__)
    try:main()
    except ImportError:print('{"code":"MISSING_PREREQUISITE","message":"Ghidra 11.4/PyGhidra 2.2 and existing analyzed program required"}',file=sys.stderr);sys.exit(3)
    except Exception as e:print(json.dumps({'code':'EXTRACTION_FAILED','message':type(e).__name__}),file=sys.stderr);sys.exit(5)
