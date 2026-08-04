import {zh} from "../i18n";

export type WizardStep="scope"|"scanning"|"results"|"complete";
const order:WizardStep[]=["scope","scanning","results","complete"];

export function WizardSteps({step}:{step:WizardStep}){
 const active=order.indexOf(step);
 return <nav className="stepper" aria-label="清理步骤">{zh.steps.map((label,index)=><div className={`step ${index===active?"active":""} ${index<active?"done":""}`} key={label}><span>{index<active?"✓":index+1}</span><b>{label}</b></div>)}</nav>;
}
