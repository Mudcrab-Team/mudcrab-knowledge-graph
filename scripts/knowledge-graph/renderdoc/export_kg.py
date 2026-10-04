#!/usr/bin/env python3
"""Selected existing-capture extraction using RenderDoc 1.43 replay APIs. No CPU/GPU guesses."""
import argparse
import json
import pathlib
import re
import sys
import time
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
from kg_bundle import Builder, canonical, digest, file_hash


def resource_id(resource):
    # ResourceId is opaque in the Python API; its documented display is ResourceId::<uint64>.
    token=str(resource);match=re.fullmatch(r'(?:ResourceId::)?(0x[0-9a-fA-F]+|[0-9]+)',token)
    if not match:raise ValueError('unsupported RenderDoc ResourceId representation')
    value=match.group(1);return hex(int(value,16 if value.startswith('0x') else 10))
def selection(v):
    allowed={'schema_version','source','events','stages','bindings','constants','max_events','max_bindings','max_constants','max_bytes','max_seconds','synthetic'}
    if v.get('schema_version')!=1 or set(v)-allowed:raise ValueError('invalid capture selection')
    cfg={'stages':['Vertex','Pixel'],'bindings':[],'constants':[],'max_events':100,'max_bindings':1000,'max_constants':256,'max_bytes':32*1024*1024,'max_seconds':120,**v}
    for k,lo,hi in [('max_events',1,100),('max_bindings',1,10000),('max_constants',1,1024),('max_bytes',1024,128*1024*1024),('max_seconds',1,600)]:
        if type(cfg[k]) is not int or not lo<=cfg[k]<=hi:raise ValueError('invalid bounded capture option')
    events=cfg.get('events',[])
    if not events or len(events)>cfg['max_events'] or len(set(events))!=len(events) or any(type(x) is not int or x<0 for x in events):raise ValueError('select unique bounded event IDs')
    if not cfg['stages'] or len(set(cfg['stages']))!=len(cfg['stages']) or any(x not in ('Vertex','Hull','Domain','Geometry','Pixel','Compute','Task','Mesh') for x in cfg['stages']):raise ValueError('invalid shader stages')
    for c in cfg['constants']:
        if set(c)!={'stage','index','array_element'} or c['stage'] not in cfg['stages'] or type(c['index']) is not int or c['index']<0 or type(c['array_element']) is not int or c['array_element']<0:raise ValueError('invalid selected constant block')
    if len(cfg['constants'])>32:raise ValueError('constant block selection limit')
    return cfg

def constant_type(constant, depth=0):
    if depth>8:raise ValueError('reflection nesting limit')
    typ=constant.type
    return {'name':str(constant.name),'byte_offset':int(constant.byteOffset),'type':str(typ.name),'base_type':str(typ.baseType),
      'rows':int(typ.rows),'columns':int(typ.columns),'elements':int(typ.elements),'array_stride':int(typ.arrayByteStride),
      'matrix_stride':int(typ.matrixByteStride),'members':[constant_type(c,depth+1) for c in typ.members]}
def constant_value(variable, depth=0):
    if depth>8:raise ValueError('constant nesting limit')
    # Preserve the underlying union bits, avoiding rounded Python float identities.
    return {'name':str(variable.name),'type':str(variable.type),'rows':int(variable.rows),'columns':int(variable.columns),
      'union_u64_bits':[hex(int(x)) for x in variable.value.u64v],
      'members':[constant_value(c,depth+1) for c in variable.members]}
def extract(controller, cfg, capture_hash, version, rd):
    cfg=selection(cfg);start=time.monotonic();diagnostics=[];events={};resources={};action_count=0;binding_count=0;constant_count=0
    def check():
        if time.monotonic()-start>cfg['max_seconds']:raise TimeoutError('capture selection timeout')
    def walk(actions, parents=()):
        nonlocal action_count
        for a in actions:
            check();action_count+=1
            if action_count>1000000:raise ValueError('capture action inventory limit')
            if int(a.eventId) in cfg['events']:
                events[int(a.eventId)]={'event_id':int(a.eventId),'parent_events':list(parents),'name':str(a.customName),'flags':str(a.flags),
                 'num_indices':int(a.numIndices),'num_instances':int(a.numInstances),'base_vertex':int(a.baseVertex),'bindings':[],'shaders':{},'constant_values':[]}
            walk(a.children,(*parents,int(a.eventId)))
    walk(controller.GetRootActions())
    if set(events)!=set(cfg['events']):raise ValueError('selected capture event unavailable')
    api=str(controller.GetAPIProperties().pipelineType)
    for event_id,event in sorted(events.items()):
        check();controller.SetFrameEvent(event_id,True);pipe=controller.GetPipelineState()
        for stage_name in cfg['stages']:
            stage=getattr(rd.ShaderStage,stage_name);shader=pipe.GetShader(stage);sid=resource_id(shader)
            if sid=='0x0':continue
            refl=pipe.GetShaderReflection(stage)
            if refl is None:
                diagnostics.append(('UNSUPPORTED_REFLECTION','Shader reflection unavailable',{'event_id':event_id,'stage':stage_name}));continue
            shader_record={'resource_id':sid,'stage':stage_name,'entry_point':pipe.GetShaderEntryPoint(stage),'constant_blocks':[],
              'input_signature':[{'name':str(x.varName),'semantic':str(x.semanticName),'register':int(x.regIndex),'type':str(x.varType),'components':int(x.compCount)} for x in refl.inputSignature],
              'output_signature':[{'name':str(x.varName),'semantic':str(x.semanticName),'register':int(x.regIndex),'type':str(x.varType),'components':int(x.compCount)} for x in refl.outputSignature]}
            for i,block in enumerate(refl.constantBlocks):
                shader_record['constant_blocks'].append({'index':i,'name':str(block.name),'byte_size':int(block.byteSize),'buffer_backed':bool(block.bufferBacked),
                  'fixed_bind_number':int(block.fixedBindNumber),'fixed_bind_set_or_space':int(block.fixedBindSetOrSpace),'variables':[constant_type(v) for v in block.variables]})
            event['shaders'][stage_name]=shader_record
            descriptors=[('read_only',pipe.GetReadOnlyResources(stage)),('read_write',pipe.GetReadWriteResources(stage)),('constant',pipe.GetConstantBlocks(stage))]
            for category,bindings in descriptors:
                for used in bindings:
                    check();access=used.access;desc=used.descriptor;index=int(access.index);array=int(access.arrayElement)
                    if cfg['bindings'] and not any(x=={'stage':stage_name,'index':index,'array_element':array} for x in cfg['bindings']):continue
                    if binding_count>=cfg['max_bindings']:
                        diagnostics.append(('BINDING_LIMIT','Selected bindings truncated',{'event_id':event_id}));break
                    rid=resource_id(desc.resource)
                    record={'stage':stage_name,'category':category,'reflection_index':index,'array_element':array,'resource_id':rid,'descriptor_type':str(access.type),
                      'descriptor_store':resource_id(access.descriptorStore),'descriptor_byte_offset':int(access.byteOffset),'descriptor_byte_size':int(access.byteSize),
                      'statically_unused':bool(access.staticallyUnused),'byte_offset':hex(int(desc.byteOffset)),'byte_size':hex(int(desc.byteSize)),
                      'first_mip':int(desc.firstMip),'num_mips':int(desc.numMips),'first_slice':int(desc.firstSlice),'num_slices':int(desc.numSlices),'format':str(desc.format)}
                    if category=='constant' and index<len(refl.constantBlocks):
                        block=refl.constantBlocks[index];record['fixed_bind_number']=int(block.fixedBindNumber);record['fixed_bind_set_or_space']=int(block.fixedBindSetOrSpace)
                    event['bindings'].append(record);binding_count+=1
                    if rid!='0x0':resources[rid]={'resource_id':rid,'capture_sha256':capture_hash,'identity_scope':'capture_local'}
            for c in cfg['constants']:
                if c['stage']!=stage_name:continue
                i=c['index'];array=c['array_element']
                if i>=len(refl.constantBlocks):
                    diagnostics.append(('UNAVAILABLE_CONSTANT','Selected constant block unavailable',{'event_id':event_id,**c}));continue
                used=pipe.GetConstantBlock(stage,i,array);d=used.descriptor;block=refl.constantBlocks[i]
                size=int(block.byteSize)
                if size>65536:raise ValueError('selected constant block byte limit')
                pipeline=pipe.GetComputePipelineObject() if stage_name=='Compute' else pipe.GetGraphicsPipelineObject()
                values=controller.GetCBufferVariableContents(pipeline,shader,stage,pipe.GetShaderEntryPoint(stage),i,d.resource,int(d.byteOffset),size)
                for v in values:
                    if constant_count>=cfg['max_constants']:
                        diagnostics.append(('CONSTANT_LIMIT','Selected constants truncated',{'event_id':event_id}));break
                    event['constant_values'].append({'stage':stage_name,'index':i,'array_element':array,'value':constant_value(v)});constant_count+=1
    return {'capture_sha256':capture_hash,'api':api,'tool_version':version,'events':{str(k):v for k,v in sorted(events.items())},'resources':resources,'diagnostics':diagnostics,
      'replay_status':'available','asset_native_mapping':'unresolved; no CPU/GPU correspondence inferred'}

def normalize(raw,cfg):
    cfg=selection(cfg);h=raw['capture_sha256'];b=Builder(cfg.get('source','capture-analysis'),'capture',h,digest(raw),'renderdoc-export',
      {'adapter':'renderdoc-1.43-api-v1','tool_version':raw['tool_version'],'selection':cfg},{'capture_sha256':h,'events':cfg['events'],'stages':cfg['stages']},cfg.get('synthetic',False))
    art=b.artifact('capture-observations.json',canonical(raw));ev=b.evidence(art,{'kind':'json_pointer','pointer':''},'RenderDoc selected replay output','observed')
    cap=b.node('Capture',h,'selected capture',{'capture_sha256':h,'api':raw['api'],'replay_status':raw['replay_status']},[ev]);resources={}
    for rid,record in raw['resources'].items():
        resources[rid]=b.node('GpuResource',h+':'+rid,rid,record,[ev]);b.edge(resources[rid],'CAPTURED_IN',cap,{},[ev],'observed')
    for eid,event in raw['events'].items():
        ce=b.evidence(art,{'kind':'capture_event','capture_sha256':h,'event_id':int(eid),'api':raw['api']},'RenderDoc selected event','observed')
        draw=b.node('DrawEvent',h+':event:'+eid,event['name'],{**event,'capture_sha256':h,'stage':'capture'},[ce]);b.edge(draw,'CAPTURED_IN',cap,{},[ce],'observed')
        for stage,shader in event['shaders'].items():
            se=b.evidence(art,{'kind':'capture_event','capture_sha256':h,'event_id':int(eid),'api':raw['api'],'stage':stage,'resource_id':shader['resource_id']},'RenderDoc shader reflection','observed')
            sn=b.node('ShaderModule',h+':event:'+eid+':'+stage,stage+' shader',shader,[se]);b.edge(draw,'CONTAINS',sn,{},[se],'observed')
        for i,binding in enumerate(event['bindings']):
            be=b.evidence(art,{'kind':'capture_event','capture_sha256':h,'event_id':int(eid),'api':raw['api'],'stage':binding['stage'],'binding':str(binding['reflection_index'])+':'+str(binding['array_element']),'resource_id':binding['resource_id']},'RenderDoc replay descriptor binding','observed')
            bn=b.node('ShaderBinding',h+':event:'+eid+':binding:'+str(i),'binding '+str(i),binding,[be]);b.edge(draw,'HAS_BINDING',bn,{},[be],'observed')
            if binding['resource_id'] in resources:b.edge(bn,'BINDS_RESOURCE',resources[binding['resource_id']],{'observed_only':True},[be],'observed')
    for code,msg,selector in raw['diagnostics']:b.diagnostic(code,msg,selector,'capture_reflection')
    b.capability('capture_events',len(raw['events']),detail='Explicit selected events; capture-local identities')
    b.capability('capture_reflection',len(raw['events'])*len(cfg['stages']),failed=sum(x[0]=='UNSUPPORTED_REFLECTION' for x in raw['diagnostics']),detail='Input unit: selected event/stage pair; bindings/reflection/explicit constants; no native or asset mapping inferred',status='partial' if raw['diagnostics'] else 'complete')
    return b

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--capture',type=pathlib.Path,required=True);p.add_argument('--selection',type=pathlib.Path,required=True);p.add_argument('--out',type=pathlib.Path,required=True);a=p.parse_args()
    cfg=selection(json.loads(a.selection.read_text()));h=file_hash(a.capture)
    import renderdoc as rd
    cap=rd.OpenCaptureFile();controller=None
    try:
        status=cap.OpenFile(str(a.capture),'',None)
        if status!=rd.ResultCode.Succeeded:raise RuntimeError('capture opening failed')
        if not cap.LocalReplaySupport():raise RuntimeError('capture has no local replay support')
        status,controller=cap.OpenCapture(rd.ReplayOptions(),None)
        if status!=rd.ResultCode.Succeeded:raise RuntimeError('capture replay unavailable')
        raw=extract(controller,cfg,h,rd.GetVersionString(),rd)
        if file_hash(a.capture)!=h:raise RuntimeError('capture changed during export')
        print(json.dumps(normalize(raw,cfg).write(a.out,cfg['max_bytes'])))
    finally:
        if controller is not None:controller.Shutdown()
        cap.Shutdown()
if __name__=='__main__':
    from kg_bundle import supervised
    supervised(__file__)
    try:main()
    except ImportError:print('{"code":"MISSING_PREREQUISITE","message":"RenderDoc 1.43 Python environment and supported existing capture required"}',file=sys.stderr);sys.exit(3)
    except Exception as e:print(json.dumps({'code':'REPLAY_OR_EXTRACTION_FAILED','message':type(e).__name__}),file=sys.stderr);sys.exit(5)
