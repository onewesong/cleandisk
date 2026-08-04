import {beforeEach,describe,expect,it,vi} from "vitest";

const mocks=vi.hoisted(()=>({invoke:vi.fn(),channels:[] as Array<{onmessage:(event:any)=>void}>}));
vi.mock("@tauri-apps/api/core",()=>({
  invoke:mocks.invoke,
  Channel:class {onmessage=(_event:any)=>{};constructor(){mocks.channels.push(this)}},
}));
import {beginScan,cancelScan,cleanCandidates,saveSettings} from "./api";
import fixture from "../fixtures/scan-report.json";

describe("Tauri command bridge",()=>{
 beforeEach(()=>{mocks.invoke.mockReset();mocks.channels.length=0;mocks.invoke.mockResolvedValue("ok")});
 it("发送固定扫描参数并转发有序 Channel 事件",async()=>{const events:string[]=[];await beginScan(event=>events.push(event.event));expect(mocks.invoke).toHaveBeenCalledWith("begin_scan",expect.objectContaining({options:{projectRoots:null},onEvent:expect.anything()}));mocks.channels[0].onmessage({event:"progress",plugin:"a",path:"/a",found:1,bytes:2});mocks.channels[0].onmessage({event:"completed",report:{}});expect(events).toEqual(["progress","completed"]);});
 it("清理只发送扫描会话 ID 与候选 ID",async()=>{await cleanCandidates("scan",["a","b"],()=>{});expect(mocks.invoke).toHaveBeenCalledWith("clean_candidates",expect.objectContaining({scanId:"scan",candidateIds:["a","b"],onEvent:expect.anything()}));});
  it("取消与设置命令参数匹配 Rust 接口",async()=>{await cancelScan("scan");expect(mocks.invoke).toHaveBeenCalledWith("cancel_scan",{scanId:"scan"});await saveSettings(["/Users/demo/Code"]);expect(mocks.invoke).toHaveBeenCalledWith("save_settings",{input:{projectRoots:["/Users/demo/Code"]}});});
  it("与 Rust 共用固定 JSON 序列化契约",()=>{expect(fixture.scanId).toBe("fixture-scan");expect(fixture.candidates[0]).toMatchObject({category:"developer-caches",risk:"low",sizeBytes:200});});
});
