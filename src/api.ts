import { Channel, invoke } from "@tauri-apps/api/core";
import type { Category, CleanEvent, CleanReport, ScanEvent, Settings } from "./types";
export const getSettings=()=>invoke<Settings>("get_settings");
export const saveSettings=(projectRoots:string[])=>invoke<Settings>("save_settings",{input:{projectRoots}});
export function beginScan(categories:Category[],onEvent:(event:ScanEvent)=>void){const channel=new Channel<ScanEvent>();channel.onmessage=event=>{onEvent(event);if(["completed","cancelled","failed"].includes(event.event))channel.onmessage=()=>{}};return invoke<string>("begin_scan",{options:{projectRoots:null,categories},onEvent:channel});}
export const cancelScan=(scanId:string)=>invoke<void>("cancel_scan",{scanId});
export function cleanCandidates(scanId:string,candidateIds:string[],onEvent:(event:CleanEvent)=>void){const channel=new Channel<CleanEvent>();channel.onmessage=event=>{onEvent(event);if(event.event==="completed")channel.onmessage=()=>{}};return invoke<CleanReport>("clean_candidates",{scanId,candidateIds,onEvent:channel});}
