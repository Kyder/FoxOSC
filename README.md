<h1 align="center">Fox OSC</h1>

![alt text](https://github.com/Kyder/FoxOSC/blob/main/RngS/Fox%20OSCApp.png) 

This was made because VRCOSC supports only windows and also i do not like that i cannot just place different plugins into specific place and it will just works. Many times i got notification about "Some addons are out of date" and error was not on my side.

So i decided that i should make something, with help of Claude AI, i made this OSC app which runs on Rust, uses GTK4 for UI and for plugins uses WebAssembly.

So far i have two plugins.

##### Watch
<pre>Sends Hours, Minutes and Seconds from you real time, sending adresses can be configured.
</pre>

##### Boop Counter
<pre>Listens for boolean value at certain adress (Can be configured) and sends into chatbox current and total boops count, 
every two sec when booped, to prevent spaming the chat.
</pre>
