import {fireEvent,render,screen,waitFor,within} from "@testing-library/react";
import {beforeEach,describe,expect,it,vi} from "vitest";
import type {Candidate,ScanReport} from "./types";

const api=vi.hoisted(()=>({
  getSettings:vi.fn(),saveSettings:vi.fn(),beginScan:vi.fn(),cancelScan:vi.fn(),cleanCandidates:vi.fn(),
}));
vi.mock("./api",()=>api);
vi.mock("@tauri-apps/api/event",()=>({listen:vi.fn().mockResolvedValue(()=>{})}));
vi.mock("@tauri-apps/plugin-dialog",()=>({open:vi.fn()}));
import App from "./App";

const candidate=(id:string,path:string,sizeBytes:number,risk:"low"|"review",category:Candidate["category"]):Candidate=>({id,plugin:"fixture",category,risk,path,sizeBytes,modifiedAt:1_700_000_000,reason:`${path} 的原因`,action:"move_to_trash"});
const report:ScanReport={scanId:"scan-1",generatedAt:"2026-08-04T00:00:00Z",disk:{totalBytes:1_000_000,freeBytes:500_000},warnings:[],candidates:[
 candidate("large","/Users/demo/Library/Caches/large",300,"low","application-caches"),
 candidate("small","/Users/demo/Library/Caches/small",100,"low","application-caches"),
 candidate("deps","/Users/demo/Code/app/node_modules",200,"review","project-dependencies"),
]};

describe("CleanDisk UI",()=>{
 beforeEach(()=>{api.getSettings.mockResolvedValue({projectRoots:[]});api.beginScan.mockImplementation(async(callback:(event:unknown)=>void)=>{callback({event:"completed",report});return "scan-1"});});
 async function scanned(){render(<App/>);fireEvent.click(screen.getByRole("button",{name:"开始扫描"}));await screen.findByText("large");}
 it("默认不勾选，并在组内按容量降序、使用连续序号",async()=>{await scanned();const boxes=screen.getAllByRole("checkbox");expect(boxes).toHaveLength(3);boxes.forEach(box=>expect(box).not.toBeChecked());const rows=screen.getAllByText(/^(large|small|node_modules)$/);expect(rows.map(row=>row.textContent)).toEqual(["large","small","node_modules"]);expect(screen.getByText("01")).toBeInTheDocument();expect(screen.getByText("02")).toBeInTheDocument();expect(screen.getByText("03")).toBeInTheDocument();});
 it("类别筛选只显示该类别",async()=>{await scanned();fireEvent.click(screen.getByRole("button",{name:/项目依赖/}));expect(screen.getByText("node_modules")).toBeInTheDocument();expect(screen.queryByText("large")).not.toBeInTheDocument();});
 it("分组全选后用单击确认，并显示项目依赖警告",async()=>{await scanned();const section=screen.getByText("项目依赖",{selector:"h3"}).closest("section")!;fireEvent.click(within(section).getByRole("button",{name:"全选分组"}));fireEvent.click(screen.getByRole("button",{name:"移入废纸篓"}));expect(screen.getByText("确认移入废纸篓")).toBeInTheDocument();expect(screen.getByText(/之后需要通过包管理器重新安装/)).toBeInTheDocument();expect(screen.getByRole("button",{name:"确认移动"})).toBeEnabled();});
 it("搜索会重新生成连续序号",async()=>{await scanned();fireEvent.change(screen.getByPlaceholderText("搜索路径或原因"),{target:{value:"small"}});await waitFor(()=>expect(screen.getAllByRole("checkbox")).toHaveLength(1));expect(screen.getByText("01")).toBeInTheDocument();});
});
