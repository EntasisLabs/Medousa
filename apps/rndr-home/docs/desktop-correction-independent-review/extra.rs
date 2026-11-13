#[cfg(test)]
mod extra_review_tests {
 use super::*;
 use admin_model::{Desktop,ReadState,Record,Resource,Route,ReadError};
 fn settle(a:&mut App){for _ in 0..8 {a.update();}}
 fn rows(n:usize)->Vec<Record>{(0..n).map(|i|Record::new(format!("Row {i}"),(0..40).map(|_|("Status","Snapshot value".into())).collect())).collect()}
 fn dataset(a:&mut App)->Entity {a.world_mut().resource_mut::<Desktop>().unwrap().reads.insert(Resource::Runtimes,ReadState::Ready(rows(80)));desktop::navigate(a.world_mut(),Route::Data(Resource::Runtimes));settle(a);a.world().resource::<desktop::Handles>().unwrap().body}
 fn offset(a:&App,b:Entity)->[f32;2]{a.world().get::<Scroll>(b).unwrap().offset}
 fn scroll(a:&mut App,b:Entity){a.world_mut().get_mut::<Scroll>(b).unwrap().offset[1]=-200.;settle(a);assert!(offset(a,b)[1]<0.);}
 fn children(a:&App,b:Entity)->Vec<Entity>{a.world().entities_with::<Parent>().into_iter().filter(|e|a.world().get::<Parent>(*e).unwrap().entity==b).collect()}
 #[test]
 fn unrelated_begin_and_settle_retain_long_list_entities_focus_and_nonzero_scroll(){
  let mut a=app();a.update();let b=dataset(&mut a);scroll(&mut a,b);
  let e=a.world().entities_with::<Button>().into_iter().find(|e|a.world().get::<Button>(*e).unwrap().action=="record:5").unwrap();
  assert!(request_focus(a.world_mut(),e));settle(&mut a);let pos=offset(&a,b);assert!(pos[1]<0.);let entities=children(&a,b);
  let s=a.world_mut().resource_mut::<Desktop>().unwrap().begin(Resource::Health).unwrap();settle(&mut a);
  assert_eq!(children(&a,b),entities);assert_eq!(a.world().resource::<Focus>().unwrap().entity,Some(e));assert_eq!(offset(&a,b),pos);
  send(a.world_mut(),admin_backend::ReadReply{resource:Resource::Health,serial:s,result:Ok(rows(2))});settle(&mut a);
  assert_eq!(children(&a,b),entities);assert_eq!(a.world().resource::<Focus>().unwrap().entity,Some(e));assert_eq!(offset(&a,b),pos);
  assert_eq!(a.world().resource::<Desktop>().unwrap().reads[&Resource::Health],ReadState::Ready(rows(2)));
 }
 #[test]
 fn filter_detail_back_and_pending_navigation_deliberately_reset(){
  let mut a=app();a.update();let b=dataset(&mut a);scroll(&mut a,b);
  let search=a.world().resource::<desktop::Handles>().unwrap().search;
  a.world_mut().get_mut::<Label>(search).unwrap().text="Row 7".into();settle(&mut a);assert_eq!(offset(&a,b),[0.,0.]);
  dataset(&mut a);scroll(&mut a,b);desktop::action(a.world_mut(),"record:5");settle(&mut a);assert_eq!(offset(&a,b),[0.,0.]);
  scroll(&mut a,b);desktop::action(a.world_mut(),"admin:back");settle(&mut a);assert_eq!(offset(&a,b),[0.,0.]);
  scroll(&mut a,b);let s=a.world_mut().resource_mut::<Desktop>().unwrap().begin(Resource::Runtimes).unwrap();settle(&mut a);
  desktop::navigate(a.world_mut(),Route::Settings(0));settle(&mut a);
  send(a.world_mut(),admin_backend::ReadReply{resource:Resource::Runtimes,serial:s,result:Ok(rows(90))});settle(&mut a);
  desktop::navigate(a.world_mut(),Route::Data(Resource::Runtimes));settle(&mut a);assert_eq!(offset(&a,b),[0.,0.]);
 }
 #[test]
 fn loading_error_clamps_and_filter_during_loading_discards_retained_offset(){
  let mut a=app();a.update();let b=dataset(&mut a);scroll(&mut a,b);
  let s=a.world_mut().resource_mut::<Desktop>().unwrap().begin(Resource::Runtimes).unwrap();settle(&mut a);
  send(a.world_mut(),admin_backend::ReadReply{resource:Resource::Runtimes,serial:s,result:Err(ReadError::Unavailable)});settle(&mut a);assert_eq!(offset(&a,b),[0.,0.]);
  dataset(&mut a);scroll(&mut a,b);let s=a.world_mut().resource_mut::<Desktop>().unwrap().begin(Resource::Runtimes).unwrap();settle(&mut a);
  let search=a.world().resource::<desktop::Handles>().unwrap().search;a.world_mut().get_mut::<Label>(search).unwrap().text="Row".into();settle(&mut a);
  send(a.world_mut(),admin_backend::ReadReply{resource:Resource::Runtimes,serial:s,result:Ok(rows(90))});settle(&mut a);assert_eq!(offset(&a,b),[0.,0.]);
 }
}
