"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, patch, type Course, type Section } from "@/lib/api";

export const courseKey = (id: string) => ["instructor-course", id] as const;
export const curriculumKey = (id: string) => ["instructor-curriculum", id] as const;

export function useCourse(id: string) {
  return useQuery({ queryKey: courseKey(id), queryFn: () => api<Course>(`/instructor/courses/${id}`) });
}

export function useCurriculum(id: string) {
  return useQuery({
    queryKey: curriculumKey(id),
    queryFn: () => api<Section[]>(`/instructor/courses/${id}/curriculum`),
  });
}

/** PATCH the course and refresh the cached copy. */
export function useUpdateCourse(id: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: Partial<Course>) => patch<Course>(`/instructor/courses/${id}`, body),
    onSuccess: (course) => {
      queryClient.setQueryData(courseKey(id), course);
      queryClient.invalidateQueries({ queryKey: ["instructor-courses"] });
    },
  });
}
