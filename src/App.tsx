import {useEffect,useMemo,useState} from "react";
import {listen} from "@tauri-apps/api/event";
import {open} from "@tauri-apps/plugin-dialog";
import {beginScan,cancelScan,cleanCandidates,getSettings,saveSettings} from "./api";
import type {Category,CleanReport,ScanReport,Settings} from "./types";
import {zh} from "./i18n";
import {size} from "./format";
import {CompleteStep} from "./components/CompleteStep";
import {ResultsStep} from "./components/ResultsStep";
import {ScopeStep,selectableCategories} from "./components/ScopeStep";
import {ScanningStep,emptyProgress,type ScanProgressState} from "./components/ScanningStep";
import {WizardSteps,type WizardStep} from "./components/WizardSteps";
import "./App.css";

export default function App(){
 const [step,setStep]=useState<WizardStep>("scope");
 const [settings,setSettings]=useState<Settings>({projectRoots:[]});
 const [selectedCategories,setSelectedCategories]=useState<Set<Category>>(new Set(selectableCategories));
 const [scanId,setScanId]=useState<string|null>(null);
 const [progress,setProgress]=useState<ScanProgressState>(emptyProgress);
 const [report,setReport]=useState<ScanReport|null>(null);
 const [selected,setSelected]=useState<Set<string>>(new Set());
 const [confirm,setConfirm]=useState(false);
 const [cleaning,setCleaning]=useState(false);
 const [cleanStatus,setCleanStatus]=useState<string>(zh.cleaning);
 const [cleanReport,setCleanReport]=useState<CleanReport|null>(null);
 const [settingsOpen,setSettingsOpen]=useState(false);
 const [notice,setNotice]=useState("");
 const [error,setError]=useState("");

 useEffect(()=>{getSettings().then(setSettings).catch(value=>setError(String(value)))},[]);
 useEffect(()=>{let unlisten:undefined|(()=>void);listen("cleaning-close-blocked",()=>setError(zh.closeBlocked)).then(value=>{unlisten=value});return()=>unlisten?.()},[]);
 const picked=useMemo(()=>report?.candidates.filter(candidate=>selected.has(candidate.id))??[],[report,selected]);
 const disk=report?.disk??cleanReport?.refreshedScan.disk;

 const toggleCategory=(category:Category)=>setSelectedCategories(current=>{const next=new Set(current);next.has(category)?next.delete(category):next.add(category);return next});
 const startScan=async()=>{
  setError("");setNotice("");setReport(null);setCleanReport(null);setSelected(new Set());setProgress(emptyProgress);setStep("scanning");let terminal=false;
  try{
   const id=await beginScan([...selectedCategories],event=>{
    if(event.event==="started")setProgress(current=>({...current,totalPlugins:event.total_plugins}));
    if(event.event==="plugin_started")setProgress(current=>({...current,plugin:event.plugin,category:event.category,path:"",pluginIndex:event.plugin_index,totalPlugins:event.total_plugins,completedPlugins:event.plugin_index-1,visited:0}));
    if(event.event==="progress")setProgress({plugin:event.plugin,category:event.category,path:event.path,pluginIndex:event.plugin_index,totalPlugins:event.total_plugins,completedPlugins:event.plugin_index-1,visited:event.visited,found:event.found,bytes:event.bytes});
    if(event.event==="plugin_completed")setProgress(current=>({...current,plugin:event.plugin,category:event.category,pluginIndex:event.plugin_index,totalPlugins:event.total_plugins,completedPlugins:event.plugin_index,visited:event.visited,found:event.found,bytes:event.bytes}));
    if(event.event==="completed"){terminal=true;setScanId(null);setReport(event.report);setSelected(new Set());setStep("results")}
    if(event.event==="cancelled"){terminal=true;setScanId(null);setStep("scope");setNotice(zh.scanCancelled)}
    if(event.event==="failed"){terminal=true;setScanId(null);setStep("scope");setError(event.message)}
   });
   if(!terminal)setScanId(id);
  }catch(value){setScanId(null);setStep("scope");setError(String(value))}
 };
 const cancel=()=>{if(scanId)cancelScan(scanId).catch(value=>setError(String(value)))};
 const backToScope=()=>{setReport(null);setSelected(new Set());setError("");setNotice("");setStep("scope")};
 const doClean=async()=>{
  if(!report||!selected.size)return;
  setConfirm(false);setCleaning(true);setError("");setCleanStatus(zh.cleaning);let done=0;
  try{const value=await cleanCandidates(report.scanId,[...selected],event=>{if(event.event==="item_completed")setCleanStatus(zh.movingProgress(++done,selected.size));if(event.event==="rescanning")setCleanStatus(zh.rescanning)});setCleanReport(value);setReport(value.refreshedScan);setSelected(new Set());setStep("complete")}catch(value){setError(String(value))}finally{setCleaning(false)}
 };
 const viewRemaining=()=>{if(cleanReport){setReport(cleanReport.refreshedScan);setSelected(new Set());setStep("results")}};
 const restart=()=>{setReport(null);setCleanReport(null);setSelected(new Set());setProgress(emptyProgress);setError("");setNotice("");setStep("scope")};
 const addRoot=async()=>{const value=await open({directory:true,multiple:false});if(typeof value==="string"&&!settings.projectRoots.includes(value)){saveSettings([...settings.projectRoots,value]).then(setSettings).catch(reason=>setError(String(reason)))}};
 const removeRoot=(root:string)=>saveSettings(settings.projectRoots.filter(value=>value!==root)).then(setSettings).catch(reason=>setError(String(reason)));

 return <div className="app"><header className="app-header"><div className="brand"><span className="logo">◒</span><div><h1>{zh.appName}</h1><p>{zh.tagline}</p></div></div><div className="header-actions"><div className="disk"><span>{zh.availableSpace}</span><b>{disk?size(disk.freeBytes):"—"}</b></div>{step==="scope"&&<button className="ghost" onClick={()=>setSettingsOpen(true)}>⚙ {zh.settings}</button>}</div></header><WizardSteps step={step}/>{error&&<div className="error-banner">{error}</div>}{notice&&<div className="notice-banner">{notice}</div>}<main className="app-main">{step==="scope"&&<ScopeStep selected={selectedCategories} settings={settings} onToggle={toggleCategory} onAll={()=>setSelectedCategories(new Set(selectableCategories))} onClear={()=>setSelectedCategories(new Set())} onStart={startScan} onManageRoots={()=>setSettingsOpen(true)}/>} {step==="scanning"&&<ScanningStep progress={progress} onCancel={cancel}/>} {step==="results"&&report&&<ResultsStep report={report} selected={selected} onSelected={setSelected} onBack={backToScope} onClean={()=>setConfirm(true)} cleaning={cleaning}/>} {step==="complete"&&cleanReport&&<CompleteStep report={cleanReport} onRemaining={viewRemaining} onRestart={restart}/>}</main>
 {confirm&&<div className="overlay"><div className="modal"><h2>{zh.confirmTitle}</h2><div className="summary"><b>{picked.length} {zh.itemUnit}</b><strong>{size(picked.reduce((sum,item)=>sum+item.sizeBytes,0))}</strong></div><p>{zh.confirmSummary(picked.filter(item=>item.risk==="low").length,picked.filter(item=>item.risk==="review").length)}</p>{picked.some(item=>item.category==="project-dependencies")&&<div className="warning">{zh.dependencyWarning}</div>}<div className="modal-actions"><button onClick={()=>setConfirm(false)}>{zh.close}</button><button className="primary" onClick={doClean}>{zh.confirm}</button></div></div></div>}
 {cleaning&&<div className="overlay"><div className="cleaning-card"><div className="spinner"/><h2>{cleanStatus}</h2><p>{zh.cleaningHint}</p></div></div>}
 {settingsOpen&&<div className="overlay"><div className="modal settings-modal"><h2>{zh.settingsTitle}</h2><p>{zh.settingsHint}</p><div className="roots">{settings.projectRoots.map(root=><div key={root}><code>{root}</code><button onClick={()=>removeRoot(root)}>{zh.remove}</button></div>)}</div><button onClick={addRoot}>{zh.addRoot}</button><div className="modal-actions"><button className="primary" onClick={()=>setSettingsOpen(false)}>{zh.done}</button></div></div></div>}
 </div>;
}
