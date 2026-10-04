#!/usr/bin/env python3
"""Actual rmcp stdio handshake/discovery/query equivalence and rejected escalation."""
import argparse,json,pathlib,selectors,subprocess,time

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',type=pathlib.Path,required=True);p.add_argument('--root',type=pathlib.Path,required=True);p.add_argument('--context',required=True);p.add_argument('--out',type=pathlib.Path,required=True);p.add_argument('--query',default='Store');a=p.parse_args()
 cli=[str(a.binary),'--root',str(a.root),'--context',a.context]
 expected=json.loads(subprocess.run([*cli,'search','--query',a.query,'--kind','CodeSymbol','--limit','2','--json'],check=True,capture_output=True,text=True,timeout=30).stdout)
 server=subprocess.Popen([*cli,'serve','--transport','stdio'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
 selector=selectors.DefaultSelector();selector.register(server.stdout,selectors.EVENT_READ);buffer=b''
 def send(message):server.stdin.write(json.dumps(message).encode()+b'\n');server.stdin.flush()
 def receive(id):
  nonlocal buffer
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   if b'\n' not in buffer:
    if not selector.select(max(0,deadline-time.monotonic())):raise TimeoutError('MCP response timeout')
    data=server.stdout.read1(65536)
    if not data:raise RuntimeError('MCP closed')
    buffer+=data
   while b'\n' in buffer:
    line,buffer=buffer.split(b'\n',1);message=json.loads(line)
    if message.get('id')==id:return message
  raise TimeoutError('MCP response timeout')
 def request(id,method,params):send({'jsonrpc':'2.0','id':id,'method':method,'params':params});return receive(id)
 try:
  initialized=request(1,'initialize',{'protocolVersion':'2025-06-18','capabilities':{},'clientInfo':{'name':'original-synthetic-smoke','version':'1'}})
  assert 'result' in initialized;send({'jsonrpc':'2.0','method':'notifications/initialized'})
  discovered=request(2,'tools/list',{});tools=discovered['result']['tools'];assert len(tools)==7
  assert all(t['annotations']['readOnlyHint'] is True and t['inputSchema']['additionalProperties'] is False for t in tools)
  output=request(3,'tools/call',{'name':'kg.search','arguments':{'query':a.query,'kinds':['CodeSymbol'],'limit':2}})
  envelope=output['result']['structuredContent'];assert envelope==expected
  rejected=[]
  for id,(name,args) in enumerate([('kg.sql',{'sql':'DELETE FROM nodes'}),('kg.search',{'query':'Material','context':'other'}),('kg.search',{'query':'Material','policy':'research-local'}),('kg.evidence',{'evidence_id':'ev_'+'a'*64,'path':'/etc/passwd'})],4):
   result=request(id,'tools/call',{'name':name,'arguments':args});assert 'error' in result;rejected.append({'tool':name,'code':result['error'].get('data',{}).get('code')})
  report={'schema_version':1,'transport':'rmcp-stdio','context_id':envelope['context_id'],'tools':[t['name'] for t in tools],'cli_equivalent':True,'rejected':rejected,'example_response':envelope}
  a.out.write_text(json.dumps(report,indent=2));print(json.dumps({'transport':'rmcp-stdio','tools':len(tools),'cli_equivalent':True,'rejected':len(rejected)}))
 finally:
  server.stdin.close()
  try:server.wait(timeout=5)
  except subprocess.TimeoutExpired:server.kill();server.wait()
if __name__=='__main__':main()
