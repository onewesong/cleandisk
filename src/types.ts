export type Risk = "low" | "review";
export type Category = "application-caches" | "developer-caches" | "project-dependencies" | "logs" | "crash-reports" | "download-leftovers" | "other";
export interface Candidate { id:string; plugin:string; category:Category; risk:Risk; path:string; sizeBytes:number; modifiedAt:number; reason:string; action:string }
export interface DiskStats { totalBytes:number; freeBytes:number }
export interface ScanReport { scanId:string; generatedAt:string; disk:DiskStats; candidates:Candidate[]; warnings:string[] }
export interface Settings { projectRoots:string[] }
export type ScanEvent =
 |{event:"started";scan_id:string;total_plugins:number}
 |{event:"plugin_started";plugin:string;category:Category;plugin_index:number;total_plugins:number}
 |{event:"progress";plugin:string;category:Category;path:string;plugin_index:number;total_plugins:number;visited:number;found:number;bytes:number}
 |{event:"plugin_completed";plugin:string;category:Category;plugin_index:number;total_plugins:number;visited:number;found:number;bytes:number}
 |{event:"completed";report:ScanReport}
 |{event:"cancelled";scan_id:string}
 |{event:"failed";scan_id:string;message:string};
export type CleanEvent = {event:"started";total:number}|{event:"item_completed";id:string;success:boolean;message:string}|{event:"rescanning"}|{event:"completed"};
export interface CleanFailure { id:string;path:string;message:string;sizeBytes:number }
export interface CleanReport { movedCount:number;movedBytes:number;failedBytes:number;failures:CleanFailure[];freeBefore:number;freeAfter:number;trashBefore:number;trashAfter:number;refreshedScan:ScanReport }
