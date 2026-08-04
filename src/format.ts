export const size=(n:number)=>{const units=["B","KiB","MiB","GiB","TiB"];let index=0,value=n;while(value>=1024&&index<units.length-1){value/=1024;index++}return `${value.toFixed(1)} ${units[index]}`};
export const deltaSize=(n:number)=>`${n>=0?"+":"−"}${size(Math.abs(n))}`;
