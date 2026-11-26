//! YouTube-specific ad-shield detection and client-side scriptlet.

pub fn is_youtube_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("youtube.com") || lower.contains("youtu.be")
}

pub fn youtube_ad_shield_script() -> &'static str {
    r#"(()=>{
if(window.__anarc_ublock_yt)return;
window.__anarc_ublock_yt=true;
try{
const s=document.createElement('style');
s.id='anarc-ublock-yt';
s.textContent=`
.video-ads, .ytp-ad-module, .ytp-ad-overlay-container, .ytp-ad-player-overlay,
.ytp-ad-action-interstitial, .ytp-ad-skip-button-slot, .ytp-ad-preview-container,
ytd-ad-slot-renderer, #masthead-ad, ytd-banner-promo-renderer,
ytd-in-feed-ad-layout-renderer, ytd-promoted-video-renderer,
ytd-display-ad-renderer, #player-ads, ytd-statement-banner-renderer,
ytd-popup-container:has(ytd-enforcement-message-view-model),
tp-yt-paper-dialog:has(#feedback), .ytd-merch-shelf-renderer,
#companion{display:none!important}
.ad-showing video{opacity:0!important}
`;
(document.head||document.documentElement).appendChild(s);

function scrub(obj){
if(!obj||typeof obj!=='object')return;
delete obj.adPlacements;
delete obj.playerAds;
delete obj.adSlots;
if(obj.playbackTracking)delete obj.playbackTracking.adPlacements;
}
let _ytpr=window.ytInitialPlayerResponse;
if(_ytpr)scrub(_ytpr);
try{
Object.defineProperty(window,'ytInitialPlayerResponse',{
configurable:true,
get(){return _ytpr;},
set(v){scrub(v);_ytpr=v;}
});
}catch(_){}
if(!window.__anarc_ytFetchHooked){
window.__anarc_ytFetchHooked=true;
const origFetch=window.fetch;
if(typeof origFetch==='function'){
window.fetch=function(...args){
const p=origFetch.apply(this,args);
const url=typeof args[0]==='string'?args[0]:(args[0]&&args[0].url)||'';
if(url.includes('/youtubei/v1/player')){
return p.then(res=>{
try{
const clone=res.clone();
res.json=()=>clone.json().then(data=>{scrub(data);return data;});
res.text=()=>clone.text().then(txt=>{
try{
const d=JSON.parse(txt);
scrub(d);
return JSON.stringify(d);
}catch(_){return txt;}
});
}catch(_){}
return res;
});
}
return p;
};
}
const origOpen=XMLHttpRequest.prototype.open;
XMLHttpRequest.prototype.open=function(method,url,...rest){
this.__anarc_ytUrl=typeof url==='string'?url:'';
return origOpen.apply(this,[method,url,...rest]);
};
const origDesc=Object.getOwnPropertyDescriptor(XMLHttpRequest.prototype,'responseText');
if(origDesc&&origDesc.get){
Object.defineProperty(XMLHttpRequest.prototype,'responseText',{
configurable:true,
get(){
const txt=origDesc.get.call(this);
if(this.__anarc_ytUrl&&this.__anarc_ytUrl.includes('/youtubei/v1/player')&&typeof txt==='string'&&txt.length>0){
try{
const d=JSON.parse(txt);
scrub(d);
return JSON.stringify(d);
}catch(_){}
}
return txt;
}
});
}
}

function skipAds(){
const mp=document.getElementById('movie_player')||document.querySelector('.html5-video-player');
const isAd=!!(mp&&(mp.classList.contains('ad-showing')||mp.classList.contains('ad-interrupting')));
const v=document.querySelector('video.html5-main-video')||document.querySelector('video');
if(isAd){
if(!window.__anarc_ytInAd){
window.__anarc_ytInAd=true;
window.__anarc_ytPrevMuted=v?v.muted:false;
}
if(v&&v.readyState>=2&&!isNaN(v.duration)&&isFinite(v.duration)&&v.duration>0.5&&!v.ended){
v.muted=true;
v.playbackRate=16.0;
if(v.currentTime<v.duration-0.25){
v.currentTime=Math.max(0,v.duration-0.1);
}
}
const btns=document.querySelectorAll('.ytp-ad-skip-button, .ytp-ad-skip-button-modern, .ytp-skip-ad-button, .ytp-ad-skip-button-container button');
btns.forEach(b=>b&&b.click());
}else if(window.__anarc_ytInAd){
window.__anarc_ytInAd=false;
if(v){
if(v.playbackRate>2)v.playbackRate=1.0;
if(typeof window.__anarc_ytPrevMuted==='boolean')v.muted=window.__anarc_ytPrevMuted;
}
}
// Only dismiss known ad/enforcement dialogs. A generic click on every
// tp-yt-paper-dialog button also hits user flows such as "Create a channel";
// clicking mutates the dialog, MutationObserver calls skipAds again, and the
// two form a self-sustaining busy loop that pins the renderer at 100% CPU.
const adDialogs=document.querySelectorAll('ytd-enforcement-message-view-model, tp-yt-paper-dialog:has(ytd-enforcement-message-view-model), tp-yt-paper-dialog:has(#dismiss-button)');
if(!window.__anarc_ytDismissed)window.__anarc_ytDismissed=new WeakSet();
adDialogs.forEach(d=>{
if(window.__anarc_ytDismissed.has(d))return;
const b=d.querySelector('#dismiss-button, yt-button-shape button');
if(!b)return;
window.__anarc_ytDismissed.add(d);
b.click();
});
}
setInterval(skipAds,200);
// Coalesce mutation bursts: a dialog re-render must not re-enter skipAds for
// every childList change (the polling interval already covers slow cases).
let pendingSkip=false;
const ob=new MutationObserver(()=>{
if(pendingSkip)return;
pendingSkip=true;
setTimeout(()=>{pendingSkip=false;skipAds();},200);
});
ob.observe(document.body||document.documentElement,{childList:true,subtree:true});
}catch(e){}
})();"#
}
