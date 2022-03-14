***This is work in progress.***

## What
**OMFG** is an accronym for *"Open Modding Framework (for) Games"*. 
OMFG's function is to be tool for games that rely heavily on community created content to help them implement in editor modding tools for users.

## Why
Modding exists currently in many games that allow community created content. 
"Modding" is to curate content. Things still need to be fair, feel fun to play and well designed. 
However no in editor solution exists from what I have seen. 
What is usually used is a chat server or forum where users can post suggestions. 
The problem with this is that a lot of jargon is needed to express to the author of a work how to navigate to offending game objects. 
Producing this jargon is laborious to begin with and describing how to change the offending objects is even more laborious. 
This is what I want to change.

In my mind this would be much better done by direct manipulation of game objects. 
The modder would make suggestions to an author by *changing the objects themselves* and providing a small comment to explain their rationale. 
This is very comparable to suggested edits on google docs but also pull requests on git. 

## How
Observing how the structure of levels is stored in these games reveales that most opt for text based serialization formats. 
Many of which are standard formats like JSON but often times you also see custom formats too. 
OMFG leverages this property and is what allows OMFG to be language agnostic and serlialization format agnostic. See docs for more details.

## Todo
- [ ] Better coherence
- [ ] Change some CLI components to use stdio
- [ ] Use more efficient diff format
- [ ] Optimize diff and merge algorithms
- [ ] Exhaustive testing
- [ ] Exhaustive error handling and friendly messages
- [ ] Binary Docs
- [ ] RIIR server
- [ ] Make server code in companion binary optional
- [ ] Server Docs
- [ ] Example game with full integration
