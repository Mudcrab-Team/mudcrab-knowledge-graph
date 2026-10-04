"""API-facing exporter tests. These are independent of optional installed-application smokes."""
import importlib.util
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest
import os
from types import SimpleNamespace as N
ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT))
from kg_bundle import identity,canonical,file_hash

def load(name,path):
    spec=importlib.util.spec_from_file_location(name,ROOT/path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
G=load('ghidra_export','ghidra/export_kg.py');R=load('renderdoc_export','renderdoc/export_kg.py')

def addr(n):
    return N(getOffset=lambda:n,getAddressSpace=lambda:N(getName=lambda:'ram'),add=lambda x:addr(n+x),__str__=lambda:'ram:'+hex(n))
def dtype():
    return N(getPathName=lambda:'/Material',getName=lambda:'Material',getLength=lambda:16,getSourceArchive=lambda:None,
      getComponents=lambda:[N(getOffset=lambda:0,getOrdinal=lambda:0,getFieldName=lambda:'flags',getLength=lambda:4,getDataType=lambda:N(getPathName=lambda:'/uint32'),getComment=lambda:None)])
def function(n,name):
    t=dtype()
    return N(getEntryPoint=lambda:addr(n),isExternal=lambda:False,getParameters=lambda:[],getName=lambda:name,
      getSymbol=lambda:N(getSource=lambda:'USER_DEFINED'),getSignature=lambda:name+'(Material*)',getSignatureSource=lambda:'IMPORTED',isThunk=lambda:False,
      getReturnType=lambda:t,getBody=lambda:N(getAddressRanges=lambda:[N(getMinAddress=lambda:addr(n),getMaxAddress=lambda:addr(n+15))]))
def program(rename=False):
    fs={0x1010:function(0x1010,'renamed' if rename else 'apply'),0x1030:function(0x1030,'helper')}
    direct=N(getAddress=lambda:addr(0x1011),getFlowType=lambda:N(isCall=lambda:True,isComputed=lambda:False))
    indirect=N(getAddress=lambda:addr(0x1012),getFlowType=lambda:N(isCall=lambda:True,isComputed=lambda:True))
    ref=N(getReferenceType=lambda:N(isCall=lambda:True,isData=lambda:False),getToAddress=lambda:addr(0x1030))
    return N(getImageBase=lambda:addr(0x1000),getFunctionManager=lambda:N(getFunctions=lambda x:list(fs.values()),getFunctionAt=lambda a:fs.get(a.getOffset())),
      getReferenceManager=lambda:N(getReferencesFrom=lambda a:[ref] if a.getOffset()==0x1011 else []),getListing=lambda:N(getInstructions=lambda body,fwd:[direct,indirect]),
      isChanged=lambda:False,getModificationNumber=lambda:17,getDataTypeManager=lambda:N(getDataType=lambda p:dtype()),getName=lambda:'native_probe',getLanguageID=lambda:'x86:LE:64:default',getCompilerSpec=lambda:N(getCompilerSpecID=lambda:'gcc'))

def descriptor():
    return N(access=N(index=0,arrayElement=0,type='ReadOnlyImage',descriptorStore='ResourceId::8',byteOffset=0,byteSize=16,staticallyUnused=False),
      descriptor=N(resource='ResourceId::5',byteOffset=0,byteSize=64,firstMip=0,numMips=1,firstSlice=0,numSlices=1,format='RGBA8'))
def replay():
    action=N(eventId=7,customName='synthetic draw',flags='Drawcall',numIndices=3,numInstances=1,baseVertex=0,children=[])
    block=N(name='Material',byteSize=16,bufferBacked=True,fixedBindNumber=1,fixedBindSetOrSpace=2,variables=[])
    refl=N(inputSignature=[],outputSignature=[],constantBlocks=[block]);used=descriptor();used.access.type='ConstantBuffer'
    pipe=N(GetShader=lambda stage:'ResourceId::9',GetShaderReflection=lambda stage:refl,GetShaderEntryPoint=lambda stage:'main',
      GetReadOnlyResources=lambda stage:[descriptor()],GetReadWriteResources=lambda stage:[],GetConstantBlocks=lambda stage:[used],GetConstantBlock=lambda *a:used,
      GetGraphicsPipelineObject=lambda:'ResourceId::12',GetComputePipelineObject=lambda:'ResourceId::13')
    variable=N(name='flag',type='UInt',rows=1,columns=1,value=N(u64v=[16]),members=[])
    return N(GetRootActions=lambda:[action],GetAPIProperties=lambda:N(pipelineType='Vulkan'),SetFrameEvent=lambda *a:None,GetPipelineState=lambda:pipe,
      GetCBufferVariableContents=lambda *a:[variable])

class Adapters(unittest.TestCase):
    def test_exact_identities(self):
        vectors=json.loads((ROOT.parents[1]/'research/knowledge-graph/examples/identity-vectors.json').read_text())
        for v in vectors['vectors']:self.assertEqual(identity(v['kind'],*v['parts']),v['expected'])
        with self.assertRaises(ValueError):canonical({'x':1.0})
    def test_native_actual_api_reads_and_rename(self):
        cfg=G.selection({'schema_version':1,'functions':['0x10'],'level':0,'synthetic':True})
        raw=G.extract(program(),cfg,'a'*64,{'ghidra':'test-api'})
        self.assertEqual(raw['functions']['0x10']['direct_calls'][0]['target'],'0x30')
        self.assertEqual(raw['functions']['0x10']['indirect_calls'][0]['status'],'unresolved')
        b=G.normalize(raw,cfg);edges=list(b.records['assertions'].values())
        self.assertTrue(any(x['predicate']=='CALLS_DIRECT' for x in edges));self.assertFalse(any(x['predicate']=='MAY_CALL' for x in edges))
        renamed=G.normalize(G.extract(program(True),cfg,'a'*64,{'ghidra':'test-api'}),cfg)
        self.assertNotEqual(b.snapshot_id,renamed.snapshot_id)
        left={n['logical_key'] for n in b.records['nodes'].values() if n['kind']=='BinaryFunction'}
        right={n['logical_key'] for n in renamed.records['nodes'].values() if n['kind']=='BinaryFunction'}
        self.assertEqual(left,right)
    def test_native_timeout_and_mutation(self):
        cfg=G.selection({'schema_version':1,'functions':['0x10'],'level':1,'synthetic':True})
        dec=N(decompileFunction=lambda *a:N(decompileCompleted=lambda:False,isTimedOut=lambda:True,getErrorMessage=lambda:'timeout'))
        raw=G.extract(program(),cfg,'a'*64,{},dec)
        self.assertEqual(raw['functions']['0x10']['decompilation']['status'],'timeout')
        p=program();calls=iter([1,2]);p.getModificationNumber=lambda:next(calls)
        with self.assertRaises(RuntimeError):G.extract(p,cfg,'a'*64,{},dec)
        with self.assertRaises(ValueError):G.selection({'schema_version':1,'functions':['0x10'],'max_functions':201})
    def test_native_typed_field_access_requires_pointer_def_use(self):
        cfg=G.selection({'schema_version':1,'functions':['0x10'],'level':2,'synthetic':True})
        typ=dtype();component=N(getOffset=lambda:0);typ.getComponentAt=lambda offset:component if offset==0 else None
        pointer=N(getDataType=lambda:typ,getPathName=lambda:'/Material*')
        node=N(getHigh=lambda:N(getDataType=lambda:pointer),getAddress=lambda:addr(0x2000),getSize=lambda:8,isConstant=lambda:False)
        offset=N(getHigh=lambda:None,getAddress=lambda:addr(0),getSize=lambda:8,isConstant=lambda:True,getOffset=lambda:0)
        output=N(getHigh=lambda:N(getDataType=lambda:pointer),getAddress=lambda:addr(0x2000),getSize=lambda:8,isConstant=lambda:False)
        output.equals=lambda other:other is output
        use=N(getMnemonic=lambda:'LOAD',getNumInputs=lambda:2,getInput=lambda i:output if i==1 else node,getOutput=lambda:None,getSeqnum=lambda:N(getTarget=lambda:addr(0x1011),getTime=lambda:9))
        output.getDescendants=lambda:[use]
        op=N(getMnemonic=lambda:'PTRSUB',getNumInputs=lambda:2,getInput=lambda i:node if i==0 else offset,getOutput=lambda:output,getSeqnum=lambda:N(getTarget=lambda:addr(0x1010),getTime=lambda:8))
        result=N(decompileCompleted=lambda:True,getDecompiledFunction=lambda:N(getC=lambda:'typed synthetic analysis'),getHighFunction=lambda:N(getPcodeOps=lambda:[op,use]))
        raw=G.extract(program(),cfg,'a'*64,{},N(decompileFunction=lambda *a:result))
        self.assertEqual(raw['functions']['0x10']['field_accesses'][0]['basis'],'recovered_type')
        b=G.normalize(raw,cfg);self.assertTrue(any(a['predicate']=='READS_FIELD' for a in b.records['assertions'].values()))
        limited={**cfg,'max_pcode_ops':1}
        limited_raw=G.extract(program(),limited,'a'*64,{},N(decompileFunction=lambda *a:result))
        self.assertEqual(len(limited_raw['functions']['0x10']['pcode']),1)
        self.assertFalse(limited_raw['functions']['0x10']['field_accesses'])
        node.getHigh=lambda:None
        raw=G.extract(program(),cfg,'a'*64,{},N(decompileFunction=lambda *a:result))
        self.assertFalse(raw['functions']['0x10']['field_accesses'])
    def test_capture_replay_api_selection_constants_and_scope(self):
        cfg=R.selection({'schema_version':1,'events':[7],'stages':['Pixel'],'synthetic':True,'constants':[{'stage':'Pixel','index':0,'array_element':0}]})
        raw=R.extract(replay(),cfg,'a'*64,'1.43',N(ShaderStage=N(Pixel='Pixel')))
        self.assertEqual(raw['events']['7']['constant_values'][0]['value']['union_u64_bits'],['0x10'])
        self.assertEqual(raw['events']['7']['bindings'][1]['fixed_bind_set_or_space'],2)
        b=R.normalize(raw,cfg);raw2={**raw,'capture_sha256':'b'*64};b2=R.normalize(raw2,cfg)
        ids1={n['id'] for n in b.records['nodes'].values() if n['kind']=='GpuResource'}
        ids2={n['id'] for n in b2.records['nodes'].values() if n['kind']=='GpuResource'}
        self.assertFalse(ids1&ids2)
        self.assertFalse(any(a['predicate']=='CORRESPONDS_TO' for a in b.records['assertions'].values()))
        with tempfile.TemporaryDirectory() as tmp:
            out=pathlib.Path(tmp)/'export';b.write(out);self.assertTrue((out/'nodes.jsonl').read_bytes())
            with self.assertRaises(FileExistsError):b.write(out)
    def test_capture_missing_event_reflection_and_limits(self):
        cfg=R.selection({'schema_version':1,'events':[8]})
        with self.assertRaises(ValueError):R.extract(replay(),cfg,'a'*64,'test',N())
        cfg=R.selection({'schema_version':1,'events':[7],'stages':['Pixel']});p=replay();pipe=p.GetPipelineState();pipe.GetShaderReflection=lambda stage:None
        raw=R.extract(p,cfg,'a'*64,'test',N(ShaderStage=N(Pixel='Pixel')))
        self.assertEqual(raw['diagnostics'][0][0],'UNSUPPORTED_REFLECTION')
        with self.assertRaises(ValueError):R.selection({'schema_version':1,'events':[7,7]})
    @unittest.skipUnless(os.environ.get('MUDCRAB_KG_BIN'), 'set MUDCRAB_KG_BIN for actual stream-import integration')
    def test_actual_stream_ingestion_of_api_extraction(self):
        binary=os.environ['MUDCRAB_KG_BIN']
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp)/'graph'
            def kg(*args):
                result=subprocess.run([binary,'--root',str(root),*map(str,args)],check=True,capture_output=True,text=True,timeout=60)
                return json.loads(result.stdout)
            kg('init')
            cfg=G.selection({'schema_version':1,'functions':['0x10'],'level':0,'synthetic':True})
            b=G.normalize(G.extract(program(),cfg,'a'*64,{'ghidra':'API double; installed application unavailable'}),cfg)
            out=pathlib.Path(tmp)/'native';b.write(out);first=kg('ingest','bundle','--bundle',out)
            self.assertTrue(kg('ingest','bundle','--bundle',out)['idempotent'])
            cfg=R.selection({'schema_version':1,'events':[7],'stages':['Pixel'],'synthetic':True})
            b=R.normalize(R.extract(replay(),cfg,'b'*64,'API double',N(ShaderStage=N(Pixel='Pixel'))),cfg)
            out=pathlib.Path(tmp)/'capture';b.write(out);second=kg('ingest','bundle','--bundle',out)
            self.assertGreater(first['assertions'],0);self.assertGreater(second['assertions'],0)
    def test_native_fixture_really_compiles(self):
        with tempfile.TemporaryDirectory() as tmp:
            exe=pathlib.Path(tmp)/'native-probe'
            subprocess.run(['cc','-g','-O0',str(ROOT/'fixtures/native_probe.c'),'-o',str(exe)],check=True,timeout=30)
            self.assertEqual(len(file_hash(exe)),64)
            output=subprocess.run([str(exe)],check=True,capture_output=True,text=True,timeout=10)
            self.assertIn('synthetic/color.dds: 2',output.stdout)
if __name__=='__main__':unittest.main()
