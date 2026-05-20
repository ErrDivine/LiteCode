# Marvis Code Plugin Implementation Plan

## Borrow idea from operating system 
- environment variables -> structural codebase information.
- scheduling -> dynamic task segmenting and mapping to oracle model. For now just mapping to oracle skills and mcps that works best when fit into an agent.
- cpu -> session kernel (a crate already).
- timer execution -> one turn of one model at one time for one core. Can have multiple cores. Thus making the 
- ...etc


## Main target
- Build an operating system for agents in the senario of coding. An operating system means stable management of agents and proper abstraction into some concept to serve something higher. 
- This should facilitate user experience like precise recognization of repetative tasks and atonomous asking if help is needed, auto-detection of a problem in the codebase (especially the problems the user seems to be stuck at, that said the live codebase status matters) and giving the user advice after analysis, 

## Notes
- The core thing about this system is a structrual definition of codebase status. The models are equally accessible to the status in the sense that the status is segmented and mapped to the oracle skill/mcp, so they have equal opportunity.
- segment the status with an LLM.
- change in status -> segment -> select model if the user's intent is already clear -> ask user if it should do like that -> execute plan.
