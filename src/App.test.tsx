import {act,fireEvent,render,screen,waitFor,within} from "@testing-library/react";
import {beforeEach,describe,expect,it,vi} from "vitest";
import type {Candidate,CleanReport,ScanEvent,ScanReport} from "./types";

const api=vi.hoisted(()=>({getSettings:vi.fn(),saveSettings:vi.fn(),beginScan:vi.fn(),cancelScan:vi.fn(),cleanCandidates:vi.fn()}));
vi.mock("./api",()=>api);
vi.mock("@tauri-apps/api/event",()=>({listen:vi.fn().mockResolvedValue(()=>{})}));
vi.mock("@tauri-apps/plugin-dialog",()=>({open:vi.fn()}));
import App from "./App";

const candidate=(id:string,path:string,sizeBytes:number,risk:"low"|"review",category:Candidate["category"]):Candidate=>({id,plugin:"fixture",category,risk,path,sizeBytes,modifiedAt:1_700_000_000,reason:`${path} 的原因`,action:"move_to_trash"});
const report:ScanReport={scanId:"scan-1",generatedAt:"2026-08-04T00:00:00Z",disk:{totalBytes:1_000_000,freeBytes:500_000},warnings:[],candidates:[candidate("large","/Users/demo/Library/Caches/large",300,"low","application-caches"),candidate("small","/Users/demo/Library/Caches/small",100,"low","application-caches"),candidate("deps","/Users/demo/Code/app/node_modules",200,"review","project-dependencies")]};
const cleanReport:CleanReport={movedCount:1,movedBytes:200,failedBytes:0,failures:[],freeBefore:500_000,freeAfter:500_000,trashBefore:100,trashAfter:300,refreshedScan:{...report,scanId:"scan-2",candidates:report.candidates.slice(0,2)}};
let scanEvent:(event:ScanEvent)=>void;

describe("CleanDisk 四步向导",()=>{
 beforeEach(()=>{api.getSettings.mockResolvedValue({projectRoots:["/Users/demo/Code"]});api.saveSettings.mockResolvedValue({projectRoots:[]});api.cancelScan.mockResolvedValue(undefined);api.beginScan.mockImplementation(async(_categories:unknown,callback:(event:ScanEvent)=>void)=>{scanEvent=callback;return "scan-1"});api.cleanCandidates.mockImplementation(async(_id:unknown,_ids:unknown,callback:(event:{event:string})=>void)=>{callback({event:"started"});callback({event:"item_completed"});callback({event:"rescanning"});callback({event:"completed"});return cleanReport});});
 const start=async()=>{render(<App/>);fireEvent.click(screen.getByRole("button",{name:"开始扫描"}));await screen.findByText("正在扫描所选内容")};
 const finish=async()=>{await start();scanEvent({event:"completed",report});await screen.findByText("扫描结果")};

 it("初始六类默认全选，清空后不能扫描",async()=>{render(<App/>);expect(screen.getByText("选择要扫描的内容")).toBeInTheDocument();const boxes=screen.getAllByRole("checkbox");expect(boxes).toHaveLength(6);boxes.forEach(box=>expect(box).toBeChecked());fireEvent.click(screen.getByRole("button",{name:"清空"}));expect(screen.getByRole("button",{name:"开始扫描"})).toBeDisabled();});
 it("扫描期间实时展示阶段与路径，完成前不展示候选",async()=>{await start();act(()=>{scanEvent({event:"started",scan_id:"scan-1",total_plugins:2});scanEvent({event:"plugin_started",plugin:"user-caches",category:"application-caches",plugin_index:1,total_plugins:2});scanEvent({event:"progress",plugin:"user-caches",category:"application-caches",path:"/Users/demo/Library/Caches/example",plugin_index:1,total_plugins:2,visited:7,found:2,bytes:300})});expect(screen.getByText("/Users/demo/Library/Caches/example")).toBeInTheDocument();expect(screen.getByText("7")).toBeInTheDocument();expect(screen.queryByText("large")).not.toBeInTheDocument();act(()=>{scanEvent({event:"plugin_completed",plugin:"user-caches",category:"application-caches",plugin_index:1,total_plugins:2,visited:10,found:2,bytes:300});scanEvent({event:"completed",report})});await screen.findByText("large");});
 it("取消后返回选择页并保留类别选择",async()=>{await start();fireEvent.click(screen.getByRole("button",{name:"取消扫描"}));expect(api.cancelScan).toHaveBeenCalledWith("scan-1");act(()=>scanEvent({event:"cancelled",scan_id:"scan-1"}));await screen.findByText(/扫描已取消/);screen.getAllByRole("checkbox").forEach(box=>expect(box).toBeChecked());});
 it("结果默认不勾选、按容量降序且序号连续",async()=>{await finish();const boxes=screen.getAllByRole("checkbox");expect(boxes).toHaveLength(3);boxes.forEach(box=>expect(box).not.toBeChecked());expect(screen.getAllByText(/^(large|small|node_modules)$/).map(node=>node.textContent)).toEqual(["large","small","node_modules"]);expect(screen.getByText("01")).toBeInTheDocument();expect(screen.getByText("03")).toBeInTheDocument();});
 it("项目依赖确认后进入独立完成页，并可查看剩余候选",async()=>{await finish();const section=screen.getByText("项目依赖",{selector:"h3"}).closest("section")!;fireEvent.click(within(section).getByRole("button",{name:"全选分组"}));fireEvent.click(screen.getByRole("button",{name:"移入废纸篓"}));expect(screen.getByText(/之后需要通过包管理器重新安装/)).toBeInTheDocument();fireEvent.click(screen.getByRole("button",{name:"确认移动"}));await screen.findByText("清理完成",{selector:"h2"});expect(screen.getByText(/通常不会立即释放磁盘空间/)).toBeInTheDocument();fireEvent.click(screen.getByRole("button",{name:"查看剩余候选"}));await waitFor(()=>expect(screen.getByText("扫描结果")).toBeInTheDocument());expect(screen.queryByText("node_modules")).not.toBeInTheDocument();});
});
