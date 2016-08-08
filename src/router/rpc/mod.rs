mod patterns;
pub use router::rpc::patterns::RegistrationPatternNode;

use super::{ConnectionHandler, random_id};

use router::messaging::send_message;
use messages::{Message, URI, RegisterOptions, CallOptions, InvocationDetails, YieldOptions, ResultDetails, ErrorType, Reason};
use ::{List, Dict,  MatchingPolicy, WampResult, Error, ErrorKind, ID};

impl ConnectionHandler{
    pub fn handle_register(&mut self, request_id: ID, options: RegisterOptions, procedure: URI) -> WampResult<()> {
        debug!("Responding to register message (id: {}, procedure: {})", request_id, procedure.uri);
        match self.realm {
            Some(ref realm) => {
                let mut realm = realm.lock().unwrap();
                let mut manager = &mut realm.registration_manager;
                let procedure_id = {
                    let procedure_id = match manager.registrations.register_with(&procedure, self.info.clone(), options.pattern_match.clone(), options.invocation_policy.clone()) {
                        Ok(procedure_id) => procedure_id,
                        Err(e) => return Err(Error::new(ErrorKind::ErrorReason(ErrorType::Register, request_id, e.reason())))
                    };
                    self.registered_procedures.push(procedure_id);
                    procedure_id
                };
                manager.registration_ids_to_uris.insert(procedure_id, (procedure.uri, options.pattern_match == MatchingPolicy::Prefix));
                send_message(&self.info, &Message::Registered(request_id, procedure_id))
            },
             None => {
                Err(Error::new(ErrorKind::InvalidState("Recieved a message while not attached to a realm")))
            }
        }
    }

    pub fn handle_unregister(&mut self, request_id: ID, procedure_id: ID) -> WampResult<()> {
        match self.realm {
            Some(ref realm) => {
                let mut realm = realm.lock().unwrap();
                let mut manager = &mut realm.registration_manager;
                let (procedure_uri, is_prefix) =  match manager.registration_ids_to_uris.get(&procedure_id) {
                    Some(&(ref uri, ref is_prefix)) => (uri.clone(), is_prefix.clone()),
                    None => return Err(Error::new(ErrorKind::ErrorReason(ErrorType::Unregister, request_id, Reason::NoSuchProcedure)))
                };


                let procedure_id = match manager.registrations.unregister_with(&procedure_uri, &self.info, is_prefix) {
                    Ok(procedure_id) => procedure_id,
                    Err(e) => return Err(Error::new(ErrorKind::ErrorReason(ErrorType::Unregister, request_id, e.reason())))
                };
                self.registered_procedures.retain(|id| {
                    *id != procedure_id
                });
                send_message(&self.info, &Message::Unregistered(request_id))
            },
            None => {
                Err(Error::new(ErrorKind::InvalidState("Recieved a message while not attached to a realm")))
            }
        }
    }

    pub fn handle_call(&mut self, request_id: ID, options: CallOptions, procedure: URI, args: Option<List>, kwargs: Option<Dict>) -> WampResult<()> {
         debug!("Responding to call message (id: {}, procedure: {})", request_id, procedure.uri);
         match self.realm {
             Some(ref realm) => {
                 let mut realm = realm.lock().unwrap();
                 let mut manager = &mut realm.registration_manager;
                 let invocation_id = random_id();
                 info!("Current procedure tree: {:?}", manager.registrations);
                 let  (registrant, procedure_id, policy) = match manager.registrations.get_registrant_for(procedure.clone()) {
                     Ok(registrant) => registrant,
                     Err(e) => return Err(Error::new(ErrorKind::ErrorReason(ErrorType::Call, request_id, e.reason())))
                 };
                 manager.active_calls.insert(invocation_id, (request_id, self.info.clone()));
                 let mut details = InvocationDetails::new();
                 details.procedure = if policy == MatchingPolicy::Strict {
                     None
                 } else {
                     Some(procedure)
                 };
                 let invocation_message = Message::Invocation(procedure_id, invocation_id, details, args, kwargs);
                 try!(send_message(registrant, &invocation_message));


                 Ok(())
             },
             None => {
                 Err(Error::new(ErrorKind::InvalidState("Recieved a message while not attached to a realm")))
             }
         }
    }

    pub fn handle_yield(&mut self, invocation_id: ID, _options: YieldOptions, args: Option<List>, kwargs: Option<Dict>) -> WampResult<()> {
        debug!("Responding to yield message (id: {})", invocation_id);
        match self.realm {
            Some(ref realm) => {
                let mut realm = realm.lock().unwrap();
                let mut manager = &mut realm.registration_manager;
                if let Some((call_id, callee)) = manager.active_calls.remove(&invocation_id) {
                    let result_message = Message::Result(call_id, ResultDetails::new(), args, kwargs);
                    send_message(&callee, &result_message)
                } else {
                    Err(Error::new(ErrorKind::InvalidState("Recieved a yield message for a call that wasn't sent")))
                }
            }, None => {
                Err(Error::new(ErrorKind::InvalidState("Recieved a message while not attached to a realm")))
            }
        }
    }



}
